// 给一个 Promise 加客户端超时。抽成共享 util（而非留在单个组件内）——wasm 侧 `WebNode` 的
// connect()/reserve() 对不可达地址没有内建超时，`reserve()` 甚至会随 swarm 拨号重试无限期
// 挂起（见 connection-panel.tsx 用法 + docs/packages/swarmdrop-web/README.md 的记录）。
// #77（配对）同样要调 `reserve()`，会需要同一层兜底。

export function withTimeout<T>(promise: Promise<T>, ms: number, timeoutError: unknown): Promise<T> {
  let timer: number;
  const timeout = new Promise<never>((_, reject) => {
    timer = window.setTimeout(() => reject(timeoutError), ms);
  });
  return Promise.race([promise, timeout]).finally(() => window.clearTimeout(timer));
}
