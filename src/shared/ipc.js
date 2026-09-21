import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const api = {
  bootstrap: () => invoke("bootstrap"),
  listSources: (kind) => invoke("list_capture_sources", { kind }),
  listMicrophones: () => invoke("list_microphones"),
  startRecording: (request) => invoke("start_recording", { request }),
  pauseRecording: () => invoke("pause_recording"),
  resumeRecording: () => invoke("resume_recording"),
  stopRecording: () => invoke("stop_recording"),
  recordingStatus: () => invoke("recording_status"),
  history: () => invoke("recording_history"),
  pendingRecordings: () => invoke("pending_recordings"),
  retryRecordingSave: (name) => invoke("retry_recording_save", { name }),
  openRecoveryFolder: () => invoke("open_recovery_folder"),
  openLicenseDocuments: () => invoke("open_license_documents"),
  openRecording: (path) => invoke("open_recording", { path }),
  openRecordingsFolder: () => invoke("open_recordings_folder"),
  chooseSaveFolder: () => invoke("choose_save_folder"),
  saveSettings: (settings) => invoke("save_settings", { settings }),
  shareOpen: () => invoke("share_open"),
  shareStatus: () => invoke("share_status"),
  shareStop: () => invoke("share_stop"),
  onRecordingEvent: (handler) => listen("recording-state", ({ payload }) => handler(payload)),
  onShareEvent: (handler) => listen("share-state-changed", ({ payload }) => handler(payload)),
  onShareUse: (handler) => listen("share-use-requested", handler)
};
