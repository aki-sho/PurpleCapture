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

const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((response) => response.json());
const main = targets.find((target) => target.type === "page" && target.title === "Purple Capture" && !target.url.includes("share.html"));
if (!main) throw new Error("Purple Captureのメイン画面が見つかりません。");
const client = await connect(main);
try {
  await client.send("Runtime.enable");
  const evaluation = await client.send("Runtime.evaluate", {
    expression: `(async()=>{
      const wait=milliseconds=>new Promise(resolve=>setTimeout(resolve,milliseconds));
      const until=async(check,timeout=15000)=>{
        const deadline=Date.now()+timeout;
        while(Date.now()<deadline){const value=await check();if(value)return value;await wait(250);}
        throw new Error('timeout');
      };
      document.querySelector('[data-source-type="browser"]')?.click();
      await wait(900);
      document.querySelector('#system-audio').checked=false;
      document.querySelector('#microphone').checked=false;
      const source=document.querySelector('[data-source-id="share:preview"]');
      const start=document.querySelector('#start-recording');
      if(!source||!source.classList.contains('selected')||!start||start.disabled){
        return {ok:false,phase:'ready',source:!!source,selected:!!source?.classList.contains('selected'),startEnabled:!!start&&!start.disabled};
      }
      start.click();
      const recording=await until(async()=>{
        const status=await window.__TAURI_INTERNALS__.invoke('recording_status');
        if(status.error)throw new Error(status.error);
        return status.state==='recording'?status:null;
      });
      await wait(3000);
      document.querySelector('#stop-recording').click();
      const stopped=await until(async()=>{
        const status=await window.__TAURI_INTERNALS__.invoke('recording_status');
        return status.state==='idle'?status:null;
      },30000);
      const history=await window.__TAURI_INTERNALS__.invoke('recording_history');
      return {ok:true,recording,stopped,entry:history[0]||null};
    })()`,
    awaitPromise: true,
    returnByValue: true
  });
  if (evaluation.exceptionDetails) throw new Error(evaluation.exceptionDetails.exception?.description || evaluation.exceptionDetails.text);
  const result = evaluation.result.value;
  console.log(JSON.stringify(result, null, 2));
  if (!result?.ok || !result.entry?.path || result.entry.sizeBytes <= 0) throw new Error("共有タブ録画が保存されませんでした。");
} finally {
  client.close();
}
