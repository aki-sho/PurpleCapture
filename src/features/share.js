import { api } from "../shared/ipc.js";
import { state } from "../shared/state.js";
import { showToast } from "../shared/ui.js";

const statusNode = document.querySelector("#share-status");
const openButton = document.querySelector("#share-tab");
const stopButton = document.querySelector("#stop-share");

function withTimeout(promise, milliseconds) {
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      setTimeout(() => reject(new Error("共有画面を開けませんでした。アプリを再起動して、もう一度お試しください。")), milliseconds);
    })
  ]);
}

function surfaceLabel(value) {
  if (value === "browser") return "タブ";
  if (value === "window") return "ウィンドウ";
  if (value === "monitor") return "画面";
  return "共有中";
}

export function applyShareState(share) {
  if (!share) return;
  state.share = share;
  statusNode.textContent = share.active
    ? `${share.title || "共有タブ"}を${surfaceLabel(share.displaySurface)}として共有中です。下の「録画を開始」から録画できます。`
    : share.windowOpen
      ? "共有画面が開いています。外部ブラウザで録画するタブを選択してください。"
      : "未共有です。外部ブラウザで録画するタブを手動選択してください。";
  openButton.textContent = share.active || share.windowOpen ? "共有画面を表示" : "タブを共有";
  stopButton.classList.toggle("hidden", !share.active);
}

export function initializeShare() {
  openButton.addEventListener("click", async () => {
    openButton.disabled = true;
    try {
      applyShareState(await withTimeout(api.shareOpen(), 8000));
    } catch (error) {
      statusNode.textContent = "共有画面を開けません";
      showToast(String(error), "error", 6000);
    } finally {
      openButton.disabled = false;
    }
  });
  stopButton.addEventListener("click", async () => {
    stopButton.disabled = true;
    try {
      await api.shareStop();
    } catch (error) {
      showToast(String(error), "error", 6000);
    } finally {
      stopButton.disabled = false;
    }
  });
}
