import { api } from "../shared/ipc.js";
import { escapeHtml, formatBytes, formatDuration, showToast } from "../shared/ui.js";

export async function refreshHistory() {
  const node = document.querySelector("#history-list");
  node.innerHTML = '<div class="skeleton"></div><div class="skeleton"></div>';
  try {
    const entries = await api.history();
    if (!entries.length) {
      node.innerHTML = '<div class="empty-list">録画履歴はまだありません</div>';
      return;
    }
    node.innerHTML = entries.map((entry) => `
      <article class="history-row">
        <div class="history-icon"><svg viewBox="0 0 24 24"><path d="m9 8 7 4-7 4V8Z"/><rect x="3" y="4" width="18" height="16" rx="2"/></svg></div>
        <div class="history-meta">
          <strong title="${escapeHtml(entry.fileName)}">${escapeHtml(entry.fileName)}</strong>
          <small>${escapeHtml(entry.targetName)} · ${escapeHtml(entry.createdAt)} · ${formatDuration(entry.durationSeconds)}</small>
        </div>
        <span class="history-size">${formatBytes(entry.sizeBytes)}</span>
        <div class="history-actions"><button class="button secondary open-recording" data-path="${escapeHtml(entry.path)}">開く</button></div>
      </article>
    `).join("");
    node.querySelectorAll(".open-recording").forEach((button) => {
      button.addEventListener("click", async () => {
        try { await api.openRecording(button.dataset.path); }
        catch (error) { showToast(String(error), "error"); }
      });
    });
  } catch (error) {
    node.innerHTML = `<div class="alert error">${escapeHtml(error)}</div>`;
  }
}

export function initializeHistory() {
  document.querySelector("#open-recordings-folder").addEventListener("click", async () => {
    try { await api.openRecordingsFolder(); }
    catch (error) { showToast(String(error), "error"); }
  });
}

