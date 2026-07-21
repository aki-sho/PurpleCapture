const token = location.pathname.split("/").filter(Boolean).pop();
const apiBase = `${location.origin}/api`;
const preview = document.querySelector("#preview");
const welcome = document.querySelector("#welcome");
const toolbar = document.querySelector("#toolbar");
const titleNode = document.querySelector("#title");
const statusNode = document.querySelector("#status");
const errorNode = document.querySelector("#error");
const startButton = document.querySelector("#start");
const changeButton = document.querySelector("#change");
const stopButton = document.querySelector("#stop");

let stream = null;
let peer = null;
let answerTimer = null;
let sessionTimer = null;
let connectionFailures = 0;

function options() {
  return {
    video: {
      frameRate: { ideal: 60, max: 60 },
      width: { ideal: 1920, max: 1920 },
      height: { ideal: 1080, max: 1080 },
      displaySurface: "browser"
    },
    audio: false,
    preferCurrentTab: false,
    selfBrowserSurface: "exclude",
    surfaceSwitching: "include",
    monitorTypeSurfaces: "include"
  };
}

async function request(path, method = "GET", body = null) {
  const response = await fetch(`${apiBase}/${path}/${token}`, {
    method,
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
    cache: "no-store"
  });
  if (!response.ok) throw new Error(`ローカル接続エラー (${response.status})`);
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

function showError(error) {
  const message = error?.name === "NotAllowedError"
    ? "共有がキャンセルされたか、ブラウザで許可されませんでした。"
    : `共有を開始できません: ${error?.message || String(error)}`;
  errorNode.textContent = message;
  errorNode.classList.remove("hidden");
}

function clearError() {
  errorNode.classList.add("hidden");
  errorNode.textContent = "";
}

function setSharing(active) {
  document.body.classList.toggle("sharing", active);
  welcome.classList.toggle("hidden", active);
  toolbar.classList.toggle("hidden", !active);
  statusNode.classList.toggle("hidden", !active);
}

function release(value) {
  if (value) for (const track of value.getTracks()) track.stop();
}

function closePeer() {
  if (answerTimer) clearInterval(answerTimer);
  if (sessionTimer) clearInterval(sessionTimer);
  answerTimer = null;
  sessionTimer = null;
  connectionFailures = 0;
  if (peer) peer.close();
  peer = null;
}

async function stopShare(notify = true) {
  const previous = stream;
  stream = null;
  preview.srcObject = null;
  closePeer();
  release(previous);
  setSharing(false);
  if (notify) {
    try { await request("stop", "POST", {}); }
    catch { /* Purple Capture終了後は無視 */ }
  }
}

async function connect(nextStream) {
  const track = nextStream.getVideoTracks()[0];
  if (!track) throw new Error("共有映像トラックを取得できませんでした。");
  const settings = track.getSettings();
  const connection = new RTCPeerConnection({ iceServers: [] });
  connection.addTrack(track, nextStream);
  const offer = await connection.createOffer();
  await connection.setLocalDescription(offer);
  await waitForIce(connection);
  await request("offer", "POST", {
    sdp: connection.localDescription.sdp,
    title: track.label || "共有タブ",
    displaySurface: settings.displaySurface || "unknown",
    width: settings.width || preview.videoWidth || 1,
    height: settings.height || preview.videoHeight || 1
  });
  peer = connection;
  statusNode.textContent = "Purple Captureへ接続中…";
  answerTimer = setInterval(async () => {
    if (!peer || peer.remoteDescription) return;
    try {
      const state = await request("session");
      connectionFailures = 0;
      if (state.answer) {
        await peer.setRemoteDescription({ type: "answer", sdp: state.answer });
        statusNode.textContent = "Purple Captureへ共有中";
        clearInterval(answerTimer);
        answerTimer = null;
        sessionTimer = setInterval(async () => {
          try {
            const current = await request("session");
            connectionFailures = 0;
            if (!current.active) await stopShare(false);
          } catch {
            statusNode.textContent = "Purple Captureとの接続が終了しました";
            connectionFailures += 1;
            if (connectionFailures >= 3) await stopShare(false);
          }
        }, 1000);
      }
    } catch (error) {
      statusNode.textContent = "Purple Captureとの接続を待っています";
      connectionFailures += 1;
      if (connectionFailures >= 8) await stopShare(false);
    }
  }, 350);
  track.addEventListener("ended", () => {
    if (stream === nextStream) stopShare();
  }, { once: true });
}

async function startShare() {
  clearError();
  startButton.disabled = true;
  changeButton.disabled = true;
  let nextStream;
  try {
    nextStream = await navigator.mediaDevices.getDisplayMedia(options());
    const previous = stream;
    closePeer();
    stream = nextStream;
    preview.srcObject = stream;
    await preview.play();
    release(previous);
    const track = stream.getVideoTracks()[0];
    titleNode.textContent = track.label || "共有タブ";
    setSharing(true);
    await connect(stream);
  } catch (error) {
    release(nextStream);
    if (!stream) setSharing(false);
    showError(error);
  } finally {
    startButton.disabled = false;
    changeButton.disabled = false;
  }
}

startButton.addEventListener("click", startShare);
changeButton.addEventListener("click", startShare);
stopButton.addEventListener("click", () => stopShare());
window.addEventListener("beforeunload", () => {
  closePeer();
  release(stream);
  navigator.sendBeacon(`${apiBase}/stop/${token}`, "{}");
});
