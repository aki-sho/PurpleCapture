import { api } from "../shared/ipc.js";
import { escapeHtml, formatBytes, formatDuration, showToast } from "../shared/ui.js";

export async function refreshHistory() {
  await refreshPending();
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

async function refreshPending() {
  let node = document.querySelector("#pending-recordings");
  if (!node) {
    node = document.createElement("section");
    node.id = "pending-recordings";
    document.querySelector("#history-list").before(node);
  }
  try {
    const entries = await api.pendingRecordings();
    node.innerHTML = entries.length ? `<h3>未保存の録画</h3>
      <p>保存先を選び直して再保存できます。確定前のデータはそのまま保持していますが、再生できない場合があります。</p>
      <button class="button secondary" id="open-recovery-folder">保管フォルダを開く</button>
      ${entries.map((entry) => `<article class="history-row"><div class="history-meta">
        <strong>${escapeHtml(entry.name)}</strong><small>${entry.ready ? "再保存できます" : "録画中または確定前のデータ"} · ${formatBytes(entry.sizeBytes)}</small>
        </div>${entry.ready ? `<button class="button secondary retry-save" data-name="${escapeHtml(entry.name)}">保存先を選んで再保存</button>` : ""}</article>`).join("")}` : "";
    node.querySelector("#open-recovery-folder")?.addEventListener("click", async () => {
      try { await api.openRecoveryFolder(); } catch (error) { showToast(String(error), "error"); }
    });
    node.querySelectorAll(".retry-save").forEach((button) => button.addEventListener("click", async () => {
      button.disabled = true;
      try {
        if (await api.retryRecordingSave(button.dataset.name)) {
          showToast("録画を再保存しました。");
          await refreshHistory();
        }
      } catch (error) { showToast(String(error), "error", 6000); }
      finally { button.disabled = false; }
    }));
  } catch (error) { node.textContent = `未保存の録画を確認できません: ${error}`; }
}

export function initializeHistory() {
  document.querySelector("#open-recordings-folder").addEventListener("click", async () => {
    try { await api.openRecordingsFolder(); }
    catch (error) { showToast(String(error), "error"); }
  });
}
