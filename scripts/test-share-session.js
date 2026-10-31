import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(new URL("../src/share.js", import.meta.url), "utf8")
  .replace(/^import .*;\r?\n/gm, "");
const first = { apiBase: "http://127.0.0.1:10001/api", token: "a".repeat(32) };
const second = { apiBase: "http://127.0.0.1:10002/api", token: "b".repeat(32) };
const response = (status, state = {}) => ({ status, ok: status === 200, json: async () => state });

async function receiver(fetchResponse) {
  const nodes = new Map();
  const intervals = [];
  const requests = [];
  const reports = [];
  let peerCount = 0;
  function node() {
    return {
      classList: { toggle() {}, add() {}, remove() {} },
      handlers: {}, value: "chrome", disabled: false,
      addEventListener(event, handler) { this.handlers[event] = handler; },
      append() {}, replaceChildren() {},
      getContext: () => ({ fillRect() {} })
    };
  }
  const context = {
    document: {
      body: node(), createElement: node,
      querySelector(selector) {
        if (!nodes.has(selector)) nodes.set(selector, node());
        return nodes.get(selector);
      }
    },
    window: { addEventListener() {} },
    setInterval(callback) { intervals.push(callback); return intervals.length; },
    setTimeout, clearTimeout,
    listen: async () => {},
    async invoke(command, data) {
      if (command === "share_session_info") return first;
      if (command === "share_browser_options") return [{ id: "chrome", name: "Chrome", available: true }];
      if (command === "share_report_state") { reports.push(data.report); return {}; }
      if (command === "share_launch_browser") return second;
      throw new Error(`Unexpected command: ${command}`);
    },
    async fetch(url, options) {
      requests.push({ url, options });
      return fetchResponse(url, options);
    },
    RTCPeerConnection: class {
      constructor() { peerCount += 1; throw new Error("A stale offer reached WebRTC"); }
    }
  };
  await vm.runInNewContext(`(async () => { ${source}\n })()`, context, { timeout: 2000 });
  return {
    poll: () => intervals[0](),
    launch: () => nodes.get("#launch-browser-share").handlers.click(),
    requests, reports,
    peerCount: () => peerCount
  };
}

test("expired session stops polling its token and a new share uses its new token", async () => {
  const app = await receiver((url) => url.endsWith(first.token) ? response(410) : response(200));
  await app.poll();
  const count = app.requests.length;
  await app.poll();
  assert.equal(app.requests.length, count);
  await app.launch();
  await app.poll();
  assert.equal(app.requests.at(-1).url, `${second.apiBase}/session/${second.token}`);
  assert.equal(app.reports.at(-1).sessionToken, first.token);
  assert.equal(app.peerCount(), 0);
});

for (const status of [200, 410]) {
  test(`late ${status} response from an old session cannot alter its replacement`, async () => {
    let finishOldPoll;
    const pending = new Promise((resolve) => { finishOldPoll = resolve; });
    const app = await receiver((url) => url === `${first.apiBase}/session/${first.token}`
      ? pending : response(200));
    const oldPoll = app.poll();
    await app.launch();
    const reportCount = app.reports.length;
    finishOldPoll(response(status, { active: true, offer: "stale offer", revision: 3 }));
    await oldPoll;
    await app.poll();
    assert.equal(app.requests.at(-1).url, `${second.apiBase}/session/${second.token}`);
    assert.equal(app.reports.length, reportCount);
    assert.equal(app.peerCount(), 0);
  });
}
