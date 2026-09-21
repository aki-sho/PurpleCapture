import { APP_VERSION } from "./shared/version.js";
import { api } from "./shared/ipc.js";
import { state } from "./shared/state.js";
import { showToast } from "./shared/ui.js";
import { applyRecordingState, initializeRecording, refreshSources, selectSourceType } from "./features/recording.js";
import { applyShareState, initializeShare } from "./features/share.js";
import { initializeHistory, refreshHistory } from "./features/history.js";
import { initializeSettings, loadMicrophones } from "./features/settings.js";

async function switchPage(page) {
  state.page = page;
  document.querySelectorAll(".page").forEach((node) => node.classList.toggle("active", node.id === `page-${page}`));
  document.querySelectorAll(".nav-item").forEach((node) => node.classList.toggle("active", node.dataset.page === page));
  if (page === "history") await refreshHistory();
  if (page === "recording" && state.sourceType === "browser") await refreshSources();
}

async function initialize() {
  document.querySelector("#app-version").textContent = `v${APP_VERSION}`;
  document.querySelector("#settings-version").textContent = APP_VERSION;
  try {
    const bootstrap = await api.bootstrap();
    state.settings = bootstrap.settings;
    state.recording = bootstrap.recording;
    state.share = bootstrap.share;
    if (bootstrap.settingsWarning) {
      showToast(bootstrap.settingsWarning, "error", 12000);
      const warning = document.createElement("p");
      warning.className = "alert";
      warning.textContent = bootstrap.settingsWarning;
      document.querySelector("#page-settings .page-header").after(warning);
    }
  } catch (error) {
    document.querySelector("#recording-error").textContent = String(error);
    document.querySelector("#recording-error").classList.remove("hidden");
    return;
  }

  initializeRecording();
  initializeShare();
  initializeHistory();
  initializeSettings();
  document.querySelectorAll(".nav-item").forEach((item) => item.addEventListener("click", () => switchPage(item.dataset.page)));

  await Promise.all([refreshSources(), loadMicrophones()]);
  applyRecordingState(state.recording);
  applyShareState(state.share);
  await api.onRecordingEvent((status) => {
    applyRecordingState(status);
    if (status.state === "idle" && state.page === "history") refreshHistory();
  });
  await api.onShareEvent(async (share) => {
    applyShareState(share);
    if (state.sourceType === "browser") await refreshSources();
  });
  await api.onShareUse(async () => {
    await switchPage("recording");
    await selectSourceType("browser");
  });

  setInterval(async () => {
    if (state.recording.state !== "idle") {
      try { applyRecordingState(await api.recordingStatus()); }
      catch { /* shutdown中は無視 */ }
    }
  }, 500);
}

window.addEventListener("error", (event) => showToast(event.message, "error"));
initialize();
