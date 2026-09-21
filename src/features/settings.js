import { api } from "../shared/ipc.js";
import { state } from "../shared/state.js";
import { escapeHtml, showToast } from "../shared/ui.js";

export async function loadMicrophones() {
  const select = document.querySelector("#mic-device");
  try {
    const devices = await api.listMicrophones();
    select.innerHTML = [
      '<option value="">既定のマイク</option>',
      ...devices.map((device) => `<option value="${escapeHtml(device.id)}">${escapeHtml(device.name)}</option>`)
    ].join("");
    select.value = state.settings.microphoneId ?? "";
    updateMicSummary();
  } catch (error) {
    select.innerHTML = '<option value="">取得できませんでした</option>';
  }
}

function updateMicSummary() {
  const select = document.querySelector("#mic-device");
  document.querySelector("#mic-summary").textContent = select.selectedOptions[0]?.textContent ?? "既定のマイク";
}

function reflectSettings() {
  document.querySelector("#save-path").value = state.settings.saveDirectory;
  document.querySelector("#quality").value = state.settings.quality;
  document.querySelector("#fps").value = String(state.settings.fps);
  document.querySelector("#system-audio").checked = state.settings.systemAudio;
  document.querySelector("#microphone").checked = state.settings.microphone;
  document.querySelector("#default-quality").value = state.settings.quality;
  document.querySelector("#default-fps").value = String(state.settings.fps);
}

async function persist(patch) {
  state.settings = await api.saveSettings({ ...state.settings, ...patch });
  reflectSettings();
}

export function initializeSettings() {
  reflectSettings();
  const licenseButton = document.createElement("button");
  licenseButton.className = "button secondary";
  licenseButton.textContent = "利用条件・ライセンスを開く";
  licenseButton.addEventListener("click", async () => {
    try { await api.openLicenseDocuments(); }
    catch (error) { showToast(String(error), "error"); }
  });
  document.querySelector("#page-settings").append(licenseButton);
  document.querySelector("#choose-save-path").addEventListener("click", async () => {
    try {
      const selected = await api.chooseSaveFolder();
      if (selected) await persist({ saveDirectory: selected });
    } catch (error) { showToast(String(error), "error"); }
  });
  document.querySelector("#mic-device").addEventListener("change", async (event) => {
    updateMicSummary();
    await persist({ microphoneId: event.target.value || null });
  });
  document.querySelector("#default-quality").addEventListener("change", (event) => persist({ quality: event.target.value }));
  document.querySelector("#default-fps").addEventListener("change", (event) => persist({ fps: Number(event.target.value) }));
  document.querySelector("#quality").addEventListener("change", (event) => persist({ quality: event.target.value }));
  document.querySelector("#fps").addEventListener("change", (event) => persist({ fps: Number(event.target.value) }));
  document.querySelector("#system-audio").addEventListener("change", (event) => persist({ systemAudio: event.target.checked }));
  document.querySelector("#microphone").addEventListener("change", (event) => persist({ microphone: event.target.checked }));
}
