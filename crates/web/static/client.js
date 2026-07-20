// 主线程桥：Worker 版 SwarmDrop 客户端。方法名与 WebNode 一致（全 async、返回同形状），
// 事件用 onEvent(f) 回调（替代 events() 的 ReadableStream）——调用方可在两版之间无感切换。
export class SwarmDropWorkerClient {
  static async spawn() {
    const c = new SwarmDropWorkerClient();
    c.worker = new Worker("./worker.js", { type: "module" });
    c.pending = new Map();
    c.eventHandlers = new Set();
    c.seq = 0;
    c.worker.onmessage = (m) => {
      const d = m.data;
      if (d.type === "event") {
        for (const h of [...c.eventHandlers]) {
          try { h(d.event); } catch (e) { console.error("event handler:", e); }
        }
        return;
      }
      if (d.type === "eventsError") {
        console.error("worker 事件流断裂:", d.error);
        return;
      }
      const p = c.pending.get(d.id);
      if (!p) return;
      c.pending.delete(d.id);
      if (d.ok) {
        p.resolve(d.value);
      } else {
        // 还原结构化错误：Error 承载 message，kind 挂回实例——与 Window 模式
        // catch 到的 { kind, message } 对齐（UI 可按 kind 分支）。
        const err = new Error(d.error?.message ?? String(d.error));
        err.kind = d.error?.kind ?? "unknown";
        p.reject(err);
      }
    };
    c.worker.onerror = (e) => console.error("worker error:", e.message ?? e);
    c.nodeId = await c.call("spawn");
    return c;
  }

  call(method, ...args) {
    return new Promise((resolve, reject) => {
      const id = ++this.seq;
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ id, method, args });
    });
  }

  onEvent(f) { this.eventHandlers.add(f); }
  node_id() { return this.nodeId; }
  connect(addr) { return this.call("connect", addr); }
  reserve(addr) { return this.call("reserve", addr); }
  connect_invite(invite) { return this.call("connect_invite", invite); }
  send_files(to, files) { return this.call("send_files", to, files); }
  pending_offers() { return this.call("pending_offers"); }
  accept_offer(sid) { return this.call("accept_offer", sid); }
  reject_offer(sid) { return this.call("reject_offer", sid); }
  resume(sid) { return this.call("resume", sid); }
  download_url(path) { return this.call("download_url", path); }
  async close() {
    await this.call("close");
    this.worker.terminate();
  }
}
