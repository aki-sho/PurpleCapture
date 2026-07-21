export const state = {
  page: "recording",
  sourceType: "monitor",
  sources: { monitor: [], window: [], browser: [] },
  selected: null,
  settings: null,
  recording: { state: "idle", elapsedSeconds: 0, targetName: "", error: null },
  share: { windowOpen: false, active: false, title: "共有タブ", displaySurface: "", width: 0, height: 0, hasAudio: false },
  timerHandle: null
};
