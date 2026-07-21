import { api } from "../shared/ipc.js";
import { state } from "../shared/state.js";
import { escapeHtml, formatDuration, showToast } from "../shared/ui.js";

const sourceList = document.querySelector("#source-list");
const errorBox = document.querySelector("#recording-error");
const startButton = document.querySelector("#start-recording");
const pauseButton = document.querySelector("#pause-recording");
const stopButton = document.querySelector("#stop-recording");

function iconFor(kind) {
  if (kind === "monitor") return '<svg viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8M12 17v4"/></svg>';
  if (kind === "browser") return '<svg viewBox="0 0 24 24"><rect x="3" y="5" width="18" height="14" rx="2"/><path d="M8 22h8M12 19v3M9 9l3-3 3 3M12 6v8"/></svg>';
  return '<svg viewBox="0 0 24 24"><rect x="3" y="5" width="18" height="15" rx="2"/><path d="M3 9h18"/></svg>';
}

export async function refreshSources() {
  sourceList.innerHTML = '<div class="skeleton"></div><div class="skeleton"></div><div class="skeleton"></div>';
  errorBox.classList.add("hidden");
  try {
    const sources = await api.listSources(state.sourceType);
    state.sources[state.sourceType] = sources;
    if (state.selected && state.selected.kind === state.sourceType && !sources.some((item) => item.id === state.selected.id)) {
      state.selected = null;
    }
    if (
      state.sourceType === "browser"
      && sources.length === 1
      && sources[0].id === "share:preview"
      && state.recording.state === "idle"
    ) {
      state.selected = sources[0];
    }
    renderSources();
  } catch (error) {
    sourceList.innerHTML = "";
    errorBox.textContent = String(error);
    errorBox.classList.remove("hidden");
  }
}

export function renderSources() {
  const sources = state.sources[state.sourceType] ?? [];
  if (!sources.length) {
    sourceList.innerHTML = `<div class="empty-list">${state.sourceType === "browser" ? "上の「タブを共有」を押し、外部ブラウザで録画するタブを選択してください" : "録画できる対象が見つかりません"}</div>`;
    updateSelectedTarget();
    return;
  }
  sourceList.innerHTML = sources.map((source) => `
    <button class="source-card ${state.selected?.id === source.id ? "selected" : ""}" data-source-id="${escapeHtml(source.id)}">
      <span class="check">✓</span>
      <span class="source-icon">${iconFor(source.kind)}</span>
      <strong title="${escapeHtml(source.name)}">${escapeHtml(source.name)}</strong>
      <small>${escapeHtml(source.detail || "")}</small>
    </button>
  `).join("");
  sourceList.querySelectorAll(".source-card").forEach((card) => {
    card.addEventListener("click", () => {
      state.selected = sources.find((source) => source.id === card.dataset.sourceId) ?? null;
      renderSources();
    });
  });
  updateSelectedTarget();
}

function updateSelectedTarget() {
  document.querySelector("#selected-target-name").textContent = state.selected?.name ?? "対象を選択してください";
  startButton.disabled = !state.selected || state.recording.state !== "idle";
}

function recordingRequest() {
  return {
    source: state.selected,
    fps: Number(document.querySelector("#fps").value),
    quality: document.querySelector("#quality").value,
    systemAudio: document.querySelector("#system-audio").checked,
    microphone: document.querySelector("#microphone").checked,
    microphoneId: state.settings.microphoneId || null
  };
}

async function start() {
  if (!state.selected) return;
  errorBox.classList.add("hidden");
  startButton.disabled = true;
  try {
    applyRecordingState(await api.startRecording(recordingRequest()));
  } catch (error) {
    errorBox.textContent = String(error);
    errorBox.classList.remove("hidden");
    startButton.disabled = false;
  }
}

async function pauseOrResume() {
  try {
    applyRecordingState(state.recording.state === "paused" ? await api.resumeRecording() : await api.pauseRecording());
  } catch (error) {
    showToast(String(error), "error");
  }
}

async function stop() {
  stopButton.disabled = true;
  try {
    applyRecordingState(await api.stopRecording());
    showToast("録画を保存しました。");
  } catch (error) {
    try { applyRecordingState(await api.recordingStatus()); }
    catch { /* 終了状態の再取得に失敗した場合は元のエラーを表示する */ }
    showToast(String(error), "error", 6000);
  } finally {
    stopButton.disabled = false;
  }
}

export function applyRecordingState(status) {
  if (!status) return;
  state.recording = status;
  const active = status.state === "recording" || status.state === "paused" || status.state === "stopping";
  document.querySelector("#timer").textContent = formatDuration(status.elapsedSeconds);
  document.querySelector("#recording-indicator").classList.toggle("active", active);
  document.querySelector("#recording-state-label").textContent = status.state === "paused" ? "一時停止中" : active ? "録画中" : "録画対象";
  if (active && status.targetName) document.querySelector("#selected-target-name").textContent = status.targetName;
  startButton.classList.toggle("hidden", active);
  pauseButton.classList.toggle("hidden", !active || status.state === "stopping");
  stopButton.classList.toggle("hidden", !active);
  pauseButton.textContent = status.state === "paused" ? "再開" : "一時停止";
  const sidebarDot = document.querySelector(".status-dot");
  sidebarDot.classList.toggle("recording", active);
  sidebarDot.classList.toggle("ready", !active);
  document.querySelector("#sidebar-status").textContent = status.state === "paused" ? "録画を一時停止中" : active ? "録画中" : "録画準備完了";
  if (!active) updateSelectedTarget();
  if (status.error) {
    errorBox.textContent = status.error;
    errorBox.classList.remove("hidden");
  }
}

export function initializeRecording() {
  document.querySelector("#refresh-sources").addEventListener("click", refreshSources);
  document.querySelectorAll(".source-tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      if (state.recording.state !== "idle") return;
      selectSourceType(tab.dataset.sourceType);
    });
  });
  startButton.addEventListener("click", start);
  pauseButton.addEventListener("click", pauseOrResume);
  stopButton.addEventListener("click", stop);
}

export async function selectSourceType(kind) {
  if (!["monitor", "window", "browser"].includes(kind) || state.recording.state !== "idle") return;
  state.sourceType = kind;
  state.selected = null;
  document.querySelectorAll(".source-tab").forEach((item) => {
    item.classList.toggle("active", item.dataset.sourceType === kind);
  });
  document.querySelector("#share-panel").classList.toggle("hidden", kind !== "browser");
  await refreshSources();
}
