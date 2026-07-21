const port = Number(process.argv[2] || 9223);

async function connect(target) {
  const socket = new WebSocket(target.webSocketDebuggerUrl);
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
    send(method, params = {}) {
      const id = ++sequence;
      const response = new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
      socket.send(JSON.stringify({ id, method, params }));
      return response;
    },
    close() { socket.close(); }
  };
}

async function evaluate(target, expression, awaitPromise = false) {
  const client = await connect(target);
  try {
    await client.send("Runtime.enable");
    const result = await client.send("Runtime.evaluate", {
      expression,
      awaitPromise,
      returnByValue: true
    });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
    return result.result.value;
  } finally {
    client.close();
  }
}

const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((response) => response.json());
const main = targets.find((target) => target.type === "page" && target.title === "Purple Capture" && !target.url.includes("share.html"));
const share = targets.find((target) => target.type === "page" && target.url.includes("share.html"));
if (!main || !share) throw new Error("Purple Captureのメイン画面または共有画面が見つかりません。");

const mainState = await evaluate(main, `(async()=>{
  const status=await window.__TAURI_INTERNALS__.invoke('share_status');
  document.querySelector('[data-source-type="browser"]')?.click();
  await new Promise(resolve=>setTimeout(resolve,700));
  const panel=document.querySelector('#share-panel');
  const source=document.querySelector('[data-source-id="share:preview"]');
  const start=document.querySelector('#start-recording');
  return {
    status,
    embeddedBrowserRemoved:!document.querySelector('[data-page="browser"]')&&!document.querySelector('#page-browser'),
    sharePanelVisible:!!panel&&!panel.classList.contains('hidden'),
    sourceVisible:!!source,
    sourceSelected:!!source?.classList.contains('selected'),
    recordingStartEnabled:!!start&&!start.disabled,
    statusText:document.querySelector('#share-status')?.textContent||''
  };
})()`, true);

const previewState = await evaluate(share, `(()=>{
  const canvas=document.querySelector('#share-canvas');
  const result={
    sharing:document.body.classList.contains('sharing'),
    error:document.querySelector('#share-error')?.textContent||'',
    canvasWidth:canvas?.width||0,
    canvasHeight:canvas?.height||0
  };
  if(result.canvasWidth>2&&result.canvasHeight>2){
    const sample=document.createElement('canvas');
    sample.width=64;sample.height=64;
    const context=sample.getContext('2d',{willReadFrequently:true});
    context.drawImage(canvas,0,0,64,64);
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
})()`);

console.log(JSON.stringify({ main: mainState, preview: previewState }, null, 2));
if (!mainState.embeddedBrowserRemoved || !mainState.sharePanelVisible) throw new Error("録画画面への共有操作統合が不完全です。");
if (!mainState.status?.active || !mainState.sourceVisible || !mainState.sourceSelected || !mainState.recordingStartEnabled) {
  throw new Error("共有タブを録画対象として選択できません。");
}
if (!previewState.sharing || !previewState.visiblePixels || previewState.peak <= 16) {
  throw new Error(`共有プレビューが黒い状態です: ${previewState.error}`);
}
