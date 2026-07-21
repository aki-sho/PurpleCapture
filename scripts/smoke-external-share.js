const port = Number(process.argv[2] || 9333);
const inspectOnly = process.argv.includes("--inspect");

async function cdp(target) {
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
    async send(method, params = {}) {
      const id = ++sequence;
      const response = new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
      socket.send(JSON.stringify({ id, method, params }));
      return response;
    },
    close() {
      socket.close();
    }
  };
}

const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((response) => response.json());
const sender = targets.find((target) => target.type === "page" && /\/share\/[a-f0-9]+$/.test(target.url));
const pattern = targets.find((target) => target.type === "page" && target.title === "PurpleCaptureShareTestPattern");
if (!sender || !pattern) throw new Error("共有ページまたは共有対象のテストタブが見つかりません。");

const client = await cdp(sender);
try {
  await client.send("Runtime.enable");
  if (!inspectOnly) {
    await client.send("Page.enable");
    await client.send("Page.reload", { ignoreCache: true });
    await new Promise((resolve) => setTimeout(resolve, 1500));
    let button;
    for (let attempt = 0; attempt < 50; attempt += 1) {
      const buttonResult = await client.send("Runtime.evaluate", {
        expression: "(()=>{const button=document.querySelector('#start');if(!button)return null;const rect=button.getBoundingClientRect();return {x:rect.left+rect.width/2,y:rect.top+rect.height/2,visible:rect.width>0&&rect.height>0,disabled:button.disabled};})()",
        returnByValue: true
      });
      button = buttonResult.result.value;
      if (button?.visible && !button.disabled) break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    if (!button || !button.visible || button.disabled) throw new Error("外部共有ページの開始ボタンを操作できません。");
    await client.send("Input.dispatchMouseEvent", {
      type: "mousePressed", x: button.x, y: button.y, button: "left", clickCount: 1
    });
    await client.send("Input.dispatchMouseEvent", {
      type: "mouseReleased", x: button.x, y: button.y, button: "left", clickCount: 1
    });
  }
  await new Promise((resolve) => setTimeout(resolve, 2500));
  const stateResult = await client.send("Runtime.evaluate", {
    expression: `(()=>{
      const video=document.querySelector('#preview');
      const track=video?.srcObject?.getVideoTracks?.()[0];
      const result={
        sharing:document.body.classList.contains('sharing'),
        readyState:video?.readyState||0,
        width:video?.videoWidth||0,
        height:video?.videoHeight||0,
        trackState:track?.readyState||'',
        trackLabel:track?.label||'',
        displaySurface:track?.getSettings?.().displaySurface||'',
        error:document.querySelector('#error')?.textContent||'',
        status:document.querySelector('#status')?.textContent||''
      };
      if(result.width>0&&result.height>0){
        const canvas=document.createElement('canvas');
        canvas.width=64; canvas.height=64;
        const context=canvas.getContext('2d',{willReadFrequently:true});
        context.drawImage(video,0,0,64,64);
        const pixels=context.getImageData(0,0,64,64).data;
        let visible=0, peak=0;
        for(let index=0;index<pixels.length;index+=4){
          const value=Math.max(pixels[index],pixels[index+1],pixels[index+2]);
          if(value>16) visible+=1;
          if(value>peak) peak=value;
        }
        result.visiblePixels=visible;
        result.peak=peak;
      }
      return result;
    })()`,
    returnByValue: true
  });
  const state = stateResult.result.value;
  console.log(JSON.stringify({ senderUrl: sender.url, targetTitle: pattern.title, ...state }, null, 2));
  if (!state.sharing || state.readyState < 2 || state.trackState !== "live") {
    throw new Error(`共有ストリームを取得できませんでした: ${state.error || state.status}`);
  }
  if (!state.visiblePixels || state.peak <= 16) {
    throw new Error("共有ストリームの映像が黒い状態です。");
  }
} finally {
  client.close();
}
