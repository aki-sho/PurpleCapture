import { readFileSync } from "node:fs";
import { createServer } from "node:http";

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

const testAssets = new Map([
  ["/receiver", ["text/html; charset=utf-8", readFileSync("scripts/fixtures/share-app-receiver.html")]],
  ["/synthetic", ["text/html; charset=utf-8", readFileSync("scripts/fixtures/synthetic-sender.html")]],
  ["/src/share.css", ["text/css; charset=utf-8", readFileSync("src/share.css")]],
  ["/build/frontend/share.js", ["text/javascript; charset=utf-8", readFileSync("build/frontend/share.js")]]
]);
const testServer = createServer((request, response) => {
  const asset = testAssets.get(new URL(request.url, "http://127.0.0.1").pathname);
  if (!asset) {
    response.writeHead(404).end("Not Found");
    return;
  }
  response.writeHead(200, {
    "Content-Type": asset[0],
    "Cache-Control": "no-store",
    "X-Content-Type-Options": "nosniff"
  });
  response.end(asset[1]);
});
await new Promise((resolve, reject) => {
  testServer.once("error", reject);
  testServer.listen(0, "127.0.0.1", resolve);
});
const testAddress = testServer.address();

function testUrl(route) {
  const value = new URL(`http://127.0.0.1:${testAddress.port}/${route}`);
  value.searchParams.set("api", apiBase);
  value.searchParams.set("token", token);
  return value.href;
}

const version = await fetch(`http://127.0.0.1:${port}/json/version`).then((response) => response.json());
const browser = await connect(version.webSocketDebuggerUrl);
const opened = [];
try {
  const senderTarget = await browser.send("Target.createTarget", { url: testUrl("synthetic") });
  opened.push(senderTarget.targetId);
  const receiverTarget = await browser.send("Target.createTarget", { url: testUrl("receiver") });
  opened.push(receiverTarget.targetId);

  let receiverInfo;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((response) => response.json());
    receiverInfo = targets.find((target) => target.id === receiverTarget.targetId);
    if (receiverInfo) break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (!receiverInfo) throw new Error("Purple Capture共有受信テストページを開けませんでした。");
  const receiver = await connect(receiverInfo.webSocketDebuggerUrl);
  try {
    await receiver.send("Runtime.enable");
    let state;
    for (let attempt = 0; attempt < 100; attempt += 1) {
      const evaluation = await receiver.send("Runtime.evaluate", {
        expression: `(()=>{
          const source=document.querySelector('#share-canvas');
          const report=window.__shareReport||null;
          const result={
            report,
            directShareButton:!!document.querySelector('#start-direct-share'),
            error:document.querySelector('#share-error')?.textContent||'',
            canvasWidth:source?.width||0,
            canvasHeight:source?.height||0
          };
          if(report?.active&&source?.width>2&&source?.height>2){
            const canvas=document.createElement('canvas');
            canvas.width=64; canvas.height=64;
            const context=canvas.getContext('2d',{willReadFrequently:true});
            context.drawImage(source,0,0,64,64);
            const pixels=context.getImageData(0,0,64,64).data;
            let visiblePixels=0,peak=0;
            for(let index=0;index<pixels.length;index+=4){
              const value=Math.max(pixels[index],pixels[index+1],pixels[index+2]);
              if(value>16)visiblePixels+=1;
              peak=Math.max(peak,value);
            }
            result.visiblePixels=visiblePixels;
            result.peak=peak;
          }
          return result;
        })()`,
        returnByValue: true
      });
      state = evaluation.result.value;
      if (state.report?.active && state.visiblePixels) break;
      if (state.error) break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    console.log(JSON.stringify(state, null, 2));
    if (state.error) throw new Error(`Purple Capture共有受信エラー: ${state.error}`);
    if (state.directShareButton) throw new Error("アプリ内の直接共有ボタンが残っています。");
    if (!state.report?.active || !state.visiblePixels || state.peak <= 16) {
      throw new Error("Purple Capture共有Canvasの映像が黒い状態です。");
    }
  } finally {
    receiver.close();
  }
} finally {
  for (const targetId of opened) {
    await browser.send("Target.closeTarget", { targetId }).catch(() => {});
  }
  browser.close();
  testServer.closeAllConnections();
  await new Promise((resolve) => testServer.close(resolve));
}
