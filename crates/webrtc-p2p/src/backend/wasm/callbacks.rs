//! JS 回调闭包的生命周期归口。
//!
//! wasm-bindgen 的 `Closure` 与 JS 侧挂着的 handler 是**两件事**：`Closure` 一旦 drop，
//! JS 那个函数指针就悬空，而浏览器可能在此之后才派发已排队的事件——届时调用它会抛
//! `closure invoked recursively or after being dropped`。那是 JS 侧的 Uncaught，
//! 不会 panic 掉 wasm，所以症状只是控制台随每条子流的关闭刷屏 + 事件静默丢失，
//! 极容易被当成网络问题查半天。
//!
//! 于是两条约束缺一不可：
//!
//! 1. 闭包要活到目标对象不再产生事件为止（丢早了回调静默失效）；
//! 2. **释放之前必须先解绑**（留晚了就是上面那个报错）。
//!
//! 官方 `webrtc-websys` 里的同一条修复是本仓提的 libp2p
//! [#6558](https://github.com/libp2p/rust-libp2p/pull/6558)；自研本传输时只带过来第 1 条，
//! 第 2 条漏了。现在两条一起由 [`JsCallbacks`] 承担。

use std::any::Any;

/// 注册在某个 JS 对象上的一组回调闭包。
///
/// 语义是「续命 + 到期解绑」：闭包活到本值被 drop，drop 时**先解绑再释放**。
/// 因此它该由「连接/通道本身」持有，而不是由建连流程的局部量持有——
/// 谁先死，回调就先失效。
///
/// `detach` 刻意收成参数而不是 target 类型上的 trait：解绑动作写在**注册点旁边**，
/// 加一个 handler 时两行相邻，不会漂移成「注册了但清单里没有」。
pub(crate) struct JsCallbacks<T> {
    target: T,
    detach: fn(&T),
    /// 闭包签名各不相同（`FnMut()` / `FnMut(MessageEvent)` / …），且注册之后不再访问，
    /// 整组装箱擦除即可——调用点传一个元组进来。
    _closures: Box<dyn Any>,
}

impl<T> JsCallbacks<T> {
    /// `closures` 收注册好的闭包（多个就传元组），`detach` 解绑它们全部。
    pub(crate) fn new(target: T, detach: fn(&T), closures: impl Any) -> Self {
        Self {
            target,
            detach,
            _closures: Box::new(closures),
        }
    }
}

impl<T> Drop for JsCallbacks<T> {
    fn drop(&mut self) {
        // 顺序是硬约束：先摘 handler，`_closures` 才随后随字段一起释放。
        (self.detach)(&self.target);
    }
}
