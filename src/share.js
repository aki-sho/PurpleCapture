import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const video = document.querySelector("#share-video");
const canvas = document.querySelector("#share-canvas");
const canvasContext = canvas.getContext("2d", {
  alpha: false,
  desynchronized: false,
  willReadFrequently: true
});
const empty = document.querySelector("#share-empty");
const controls = document.querySelector("#share-controls");
const titleNode = document.querySelector("#shared-title");
const errorNode = document.querySelector("#share-error");
const audioNote = document.querySelector("#audio-note");
const recordingBadge = document.querySelector("#recording-badge");
const launchButton = document.querySelector("#launch-browser-share");
const browserSelect = document.querySelector("#share-browser");
const changeButton = document.querySelector("#change-share");
const stopButton = document.querySelector("#stop-share-window");
const useButton = document.querySelector("#use-share");
const waitingMessage = document.querySelector("#waiting-message");

let session = null;
let stream = null;
let videoTrack = null;
let peer = null;
let currentRevision = -1;
let polling = false;
let renderGeneration = 0;
let renderHandle = null;
let renderHandleType = null;
let renderedMetadata = null;
let reportedFrameReady = false;

function readableError(error) {
  if (error?.name === "NotAllowedError") return "共有がキャンセルされたか、画面共有が許可されていません。";
  if (error?.name === "InvalidStateError") return "共有ボタンを直接クリックして、もう一度お試しください。";
  if (error?.name === "NotFoundError") return "共有できる画面またはタブが見つかりません。";
  return `画面共有を開始できません: ${error?.message || String(error)}`;
}

function showError(message) {
  errorNode.textContent = message;
  errorNode.classList.remove("hidden");
}

function clearError() {
  errorNode.classList.add("hidden");
  errorNode.textContent = "";
}

function renderSharing(active) {
  document.body.classList.toggle("sharing", active);
  empty.classList.toggle("hidden", active);
  controls.classList.toggle("hidden", !active);
  audioNote.classList.toggle("hidden", !active);
}

function cancelFrameRendering() {
  renderGeneration += 1;
  if (renderHandle !== null) {
    if (renderHandleType === "video" && typeof video.cancelVideoFrameCallback === "function") {
      video.cancelVideoFrameCallback(renderHandle);
    } else if (renderHandleType === "animation") {
      cancelAnimationFrame(renderHandle);
    }
  }
  renderHandle = null;
  renderHandleType = null;
  renderedMetadata = null;
  reportedFrameReady = false;
}

function scheduleFrame(generation) {
  if (generation !== renderGeneration || !video.srcObject) return;
  if (typeof video.requestVideoFrameCallback === "function") {
    renderHandleType = "video";
    renderHandle = video.requestVideoFrameCallback(() => drawFrame(generation));
  } else {
    renderHandleType = "animation";
    renderHandle = requestAnimationFrame(() => drawFrame(generation));
  }
}

async function drawFrame(generation) {
  renderHandle = null;
  if (generation !== renderGeneration || !video.srcObject) return;
  const width = video.videoWidth || renderedMetadata?.width || 0;
  const height = video.videoHeight || renderedMetadata?.height || 0;
  if (width > 0 && height > 0) {
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    canvasContext.globalCompositeOperation = "copy";
    canvasContext.drawImage(video, 0, 0, width, height);
    // willReadFrequentlyと小さなreadbackで、動画を別GPUレイヤーのままにしない。
    canvasContext.getImageData(0, 0, 1, 1);
    if (!reportedFrameReady) {
      reportedFrameReady = true;
      titleNode.textContent = renderedMetadata?.title || videoTrack?.label || "共有タブ";
      renderSharing(true);
      waitingMessage.classList.add("hidden");
      try {
        await report(true, renderedMetadata);
      } catch (error) {
        showError(readableError(error));
      }
    }
  }
  scheduleFrame(generation);
}

function startFrameRendering(metadata) {
  cancelFrameRendering();
  renderedMetadata = metadata;
  const generation = renderGeneration;
  canvasContext.fillStyle = "#000";
  canvasContext.fillRect(0, 0, canvas.width, canvas.height);
  scheduleFrame(generation);
}

async function report(active, metadata = null) {
  const settings = active && videoTrack ? videoTrack.getSettings() : {};
  return invoke("share_report_state", {
    report: {
      active,
      title: active ? (metadata?.title || videoTrack?.label || "共有タブ") : "共有タブ",
      displaySurface: active ? (metadata?.displaySurface || settings.displaySurface || "unknown") : "",
      width: active ? (metadata?.width || settings.width || video.videoWidth || 1) : 0,
      height: active ? (metadata?.height || settings.height || video.videoHeight || 1) : 0,
      hasAudio: false
    }
  });
}

function releaseStream(value) {
  if (!value) return;
  for (const track of value.getTracks()) track.stop();
}

function closePeer() {
  if (peer) peer.close();
  peer = null;
}

async function signal(path, method = "GET", body = null) {
  if (!session) throw new Error("共有セッションがありません。");
  const response = await fetch(`${session.apiBase}/${path}/${session.token}`, {
    method,
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
    cache: "no-store"
  });
  if (!response.ok) throw new Error(`ローカル共有接続エラー (${response.status})`);
  return response.json();
}

function waitForIce(connection) {
  if (connection.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    const changed = () => {
      if (connection.iceGatheringState === "complete") {
        connection.removeEventListener("icegatheringstatechange", changed);
        resolve();
      }
    };
    connection.addEventListener("icegatheringstatechange", changed);
    setTimeout(resolve, 3000);
  });
}

async function stopLocal({ notifyRust = true, notifyServer = true } = {}) {
  const previous = stream;
  stream = null;
  videoTrack = null;
  cancelFrameRendering();
  video.srcObject = null;
  closePeer();
  if (notifyServer && session) {
    try { await signal("stop", "POST", {}); }
    catch { /* 終了中は無視 */ }
  }
  releaseStream(previous);
  renderSharing(false);
  if (notifyRust) {
    try { await report(false); }
    catch (error) { showError(readableError(error)); }
  }
}

async function receiveOffer(state) {
  closePeer();
  currentRevision = state.revision;
  const connection = new RTCPeerConnection({ iceServers: [] });
  peer = connection;
  connection.addEventListener("track", async (event) => {
    if (peer !== connection) return;
    const remoteStream = event.streams[0] || new MediaStream([event.track]);
    stream = remoteStream;
    videoTrack = event.track;
    video.srcObject = remoteStream;
    await video.play();
    startFrameRendering(state);
    event.track.addEventListener("ended", () => {
      if (videoTrack === event.track) stopLocal({ notifyServer: false });
    }, { once: true });
  });
  connection.addEventListener("connectionstatechange", () => {
    if (peer !== connection) return;
    if (["failed", "closed"].includes(connection.connectionState)) {
      stopLocal({ notifyServer: false });
    }
  });
  await connection.setRemoteDescription({ type: "offer", sdp: state.offer });
  const answer = await connection.createAnswer();
  await connection.setLocalDescription(answer);
  await waitForIce(connection);
  await signal("answer", "POST", { sdp: connection.localDescription.sdp });
}

async function pollSession() {
  if (polling || !session) return;
  polling = true;
  try {
    const state = await signal("session");
    if (state.active && state.offer && state.revision !== currentRevision) {
      await receiveOffer(state);
    } else if (!state.active && stream) {
      await stopLocal({ notifyServer: false });
    }
  } catch {
    if (stream) {
      showError("外部ブラウザとのローカル接続を確認しています。");
    }
  } finally {
    polling = false;
  }
}

async function launchBrowser() {
  clearError();
  launchButton.disabled = true;
  changeButton.disabled = true;
  try {
    session = await invoke("share_launch_browser", { browser: browserSelect.value });
    waitingMessage.classList.remove("hidden");
  } catch (error) {
    showError(readableError(error));
  } finally {
    launchButton.disabled = false;
    changeButton.disabled = false;
  }
}

async function loadBrowserOptions() {
  const options = await invoke("share_browser_options");
  browserSelect.replaceChildren();
  for (const option of options) {
    const node = document.createElement("option");
    node.value = option.id;
    node.disabled = !option.available;
    node.textContent = `${option.name}${option.available ? "" : "（未検出）"}`;
    browserSelect.append(node);
  }
  const preferred = options.find((option) => option.id === "chrome" && option.available)
    || options.find((option) => option.id === "edge" && option.available)
    || options.find((option) => option.available);
  if (preferred) browserSelect.value = preferred.id;
}

function setRecordingMode(active) {
  document.body.classList.toggle("recording-mode", active);
  recordingBadge.classList.toggle("hidden", !active);
}

launchButton.addEventListener("click", launchBrowser);
changeButton.addEventListener("click", launchBrowser);
stopButton.addEventListener("click", () => stopLocal());
useButton.addEventListener("click", async () => {
  try { await invoke("share_focus_main"); }
  catch (error) { showError(readableError(error)); }
});

window.purpleCaptureShare = {
  stopFromHost: () => stopLocal({ notifyServer: false }),
  setRecordingMode
};

window.addEventListener("beforeunload", () => {
  cancelFrameRendering();
  closePeer();
  releaseStream(stream);
});

await listen("recording-state", ({ payload }) => {
  setRecordingMode(["recording", "paused", "stopping"].includes(payload?.state));
});

try {
  session = await invoke("share_session_info");
  await loadBrowserOptions();
  await report(false);
  setInterval(pollSession, 350);
} catch (error) {
  showError(readableError(error));
}
