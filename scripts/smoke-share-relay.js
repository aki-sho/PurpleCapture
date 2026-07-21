import { pathToFileURL } from "node:url";
import path from "node:path";

const port = Number(process.argv[2] || 9333);
const apiBase = process.argv[3];
const token = process.argv[4];
if (!apiBase || !token) throw new Error("API base and token are required.");

async function connect(webSocketDebuggerUrl) {
  const socket = new WebSocket(webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let sequence = 0;
  const pending = new Map();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(String(event.data));
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.error) request.reject(new Error(message.error.message));
    else request.resolve(message.result);
  });
  return {
    async send(method, params = {}) {
      const id = ++sequence;
      const response = new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
      socket.send(JSON.stringify({ id, method, params }));
      return response;
    },
    close() { socket.close(); }
  };
}

const version = await fetch(`http://127.0.0.1:${port}/json/version`).then((response) => response.json());
const browser = await connect(version.webSocketDebuggerUrl);
let targetId;
try {
  const receiverFile = path.resolve("scripts", "fixtures", "share-receiver.html");
  const receiverUrl = new URL(pathToFileURL(receiverFile));
  receiverUrl.searchParams.set("api", apiBase);
  receiverUrl.searchParams.set("token", token);
  ({ targetId } = await browser.send("Target.createTarget", { url: receiverUrl.href }));
  let receiverTarget;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((response) => response.json());
    receiverTarget = targets.find((target) => target.id === targetId);
    if (receiverTarget) break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (!receiverTarget) throw new Error("共有受信テストタブを開けませんでした。");
  const receiver = await connect(receiverTarget.webSocketDebuggerUrl);
  try {
    await receiver.send("Runtime.enable");
    let result;
    for (let attempt = 0; attempt < 80; attempt += 1) {
      const evaluation = await receiver.send("Runtime.evaluate", {
        expression: "document.querySelector('#result')?.textContent||''",
        returnByValue: true
      });
      const text = evaluation.result.value;
      if (text?.startsWith("{")) {
        result = JSON.parse(text);
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    if (!result) throw new Error("共有映像の受信がタイムアウトしました。");
    console.log(JSON.stringify(result, null, 2));
    if (result.error) throw new Error(`共有受信エラー: ${result.error}`);
    if (!result.received || !result.visiblePixels || result.peak <= 16) {
      throw new Error("共有受信後の映像が黒い状態です。");
    }
  } finally {
    receiver.close();
  }
} finally {
  if (targetId) await browser.send("Target.closeTarget", { targetId }).catch(() => {});
  browser.close();
}
