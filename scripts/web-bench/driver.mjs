#!/usr/bin/env node
// SwarmDrop Web 传输基准驱动：headless Chrome 双 tab（recv + send），CDP 裸协议（零依赖，node ≥ 22）。
// 用法：node driver.mjs <helper-ws-multiaddr-with-/p2p/id> [sizeBytes] [verify=1|0]
// 前置：crates/web 已 wasm-pack build；http.server 已在 127.0.0.1:8080 -d crates/web/static；
//       net-web-smoke helper 已运行。结果 JSON 打到 stdout（BENCH_DONE 行之后）。

import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const PORT = 9333;
const BASE = `http://127.0.0.1:${PORT}`;
const PAGE = "http://127.0.0.1:8080/bench.html";

const helper = process.argv[2];
if (!helper) { console.error("用法: node driver.mjs <helper-ws-addr>/p2p/<id> [sizeBytes] [verify]"); process.exit(1); }
const size = process.argv[3] || String(256 * 1024 * 1024);
const verify = process.argv[4] ?? "1";
const run = Date.now().toString(36);

const profile = mkdtempSync(join(tmpdir(), "swarmdrop-bench-chrome-"));
const chrome = spawn(CHROME, [
  "--headless=new",
  `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${profile}`,
  "--no-first-run", "--no-default-browser-check",
  // 关掉后台 tab 节流——两 tab 必有一个"后台"，不关会污染 rAF/定时器指标
  "--disable-background-timer-throttling",
  "--disable-renderer-backgrounding",
  "--disable-backgrounding-occluded-windows",
  "about:blank",
], { stdio: "ignore" });
process.on("exit", () => chrome.kill());

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitCdp() {
  for (let i = 0; i < 60; i++) {
    try { const r = await fetch(`${BASE}/json/version`); if (r.ok) return; } catch {}
    await sleep(250);
  }
  throw new Error("Chrome CDP 未就绪");
}

async function newTab(url) {
  // CDP 端点格式是 PUT /json/new?{url}——页面 URL 必须整体编码，否则自身 query 的 & 会被吞
  const r = await fetch(`${BASE}/json/new?${encodeURIComponent(url)}`, { method: "PUT" });
  return r.json();
}

function connectWs(url) {
  return new Promise((res, rej) => {
    const ws = new WebSocket(url);
    ws.onopen = () => res(ws);
    ws.onerror = (e) => rej(new Error("CDP ws 连接失败: " + e.message));
  });
}

let seq = 0;
function evaluate(ws, expression) {
  return new Promise((res, rej) => {
    const id = ++seq;
    const onMsg = (m) => {
      const d = JSON.parse(m.data);
      if (d.id !== id) return;
      ws.removeEventListener("message", onMsg);
      if (d.error) rej(new Error(JSON.stringify(d.error)));
      else res(d.result?.result?.value);
    };
    ws.addEventListener("message", onMsg);
    ws.send(JSON.stringify({ id, method: "Runtime.evaluate", params: { expression, returnByValue: true } }));
  });
}

await waitCdp();
const params = (role) =>
  `role=${role}&run=${run}&size=${size}&verify=${verify}&helper=${encodeURIComponent(helper)}`;

console.log(`# run=${run} size=${size} verify=${verify}`);
const recvTab = await newTab(`${PAGE}?${params("recv")}`);
await sleep(2000); // recv 先 spawn + reserve
const sendTab = await newTab(`${PAGE}?${params("send")}`);

const wsRecv = await connectWs(recvTab.webSocketDebuggerUrl);
const wsSend = await connectWs(sendTab.webSocketDebuggerUrl);

// 抓 console error / 未捕获异常（wasm panic 详情走 console.error）
function attachConsole(ws, tag) {
  ws.addEventListener("message", (m) => {
    const d = JSON.parse(m.data);
    if (d.method === "Runtime.consoleAPICalled") {
      const text = d.params.args.map((a) => a.value ?? a.description ?? "").join(" ");
      if (d.params.type === "error" || /panicked|RuntimeError|unreachable/.test(text))
        console.log(`{${tag}:console.${d.params.type}} ${text.slice(0, 800)}`);
    } else if (d.method === "Runtime.exceptionThrown") {
      const e = d.params.exceptionDetails;
      console.log(`{${tag}:exception} ${(e.exception?.description ?? e.text ?? "").slice(0, 800)}`);
    }
  });
  ws.send(JSON.stringify({ id: ++seq, method: "Runtime.enable" }));
}
attachConsole(wsRecv, "recv");
attachConsole(wsSend, "send");

// 轮询结果，顺带回放两侧新增日志
const seen = { recv: 0, send: 0 };
async function drainLog(ws, tag) {
  const lines = (await evaluate(ws, "window.__benchLog")) ?? [];
  for (; seen[tag] < lines.length; seen[tag]++) console.log(lines[seen[tag]]);
  return lines;
}

const deadline = Date.now() + 20 * 60 * 1000;
let recvResult = null, sendResult = null;
while (Date.now() < deadline && (!recvResult || !sendResult)) {
  await sleep(2000);
  try {
    await drainLog(wsRecv, "recv");
    await drainLog(wsSend, "send");
    if (!recvResult) recvResult = await evaluate(wsRecv, "window.__benchResult");
    if (!sendResult) sendResult = await evaluate(wsSend, "window.__benchResult");
  } catch (e) { console.error("轮询失败: " + e.message); }
}

console.log("BENCH_DONE");
console.log(JSON.stringify({ run, size: Number(size), recvResult, sendResult }, null, 2));
chrome.kill();
process.exit(recvResult?.ok && sendResult?.ok ? 0 : 1);
