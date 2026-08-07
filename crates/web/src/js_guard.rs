//! JS 事件回调的 RAII 句柄。
//!
//! wasm-bindgen 的 `Closure` 与 JS 侧挂着的 handler 是**两件事**：`Closure` 一旦释放，
//! JS 那个函数指针就悬空，之后到达的事件会抛
//! `closure invoked recursively or after being dropped`——JS 侧的 Uncaught，不会 panic
//! 掉 wasm，症状只有控制台刷屏 + 事件静默丢失。故两条约束缺一不可：闭包要活到目标对象
//! 不再产生事件，**且释放之前必须先解绑**。
//!
//! 两种解绑机制（`removeEventListener` 与 `onxxx = null`）的差别只在 `detach` 那一行，
//! 因此收成同一个类型：`detach` 拿得到 target 与闭包本体，前者需要闭包引用，后者不需要。

/// 目标对象 + 注册在它上面的闭包，drop 时**先解绑再释放**。
///
/// `closures` 是单个 `Closure` 或它们的数组/元组——注册几个就传几个，
/// `detach` 负责把它们全部摘掉。
///
/// `#[must_use]` 不是礼节：写成裸语句（或 `let _ = …`）会让 guard 当场 drop，回调随即
/// 失效——`install_upgrade_handler` 那处就会变成「一张 object store 都建不出来」。
/// 有 `Drop` impl **不会**触发 unused-result lint，只能靠这个属性拦。
#[must_use = "guard 一 drop 回调就解绑失效，必须持有到目标对象不再产生事件"]
pub struct JsGuard<T, C> {
    target: T,
    closures: C,
    detach: fn(&T, &C),
}

impl<T, C> JsGuard<T, C> {
    pub fn new(target: T, closures: C, detach: fn(&T, &C)) -> Self {
        Self {
            target,
            closures,
            detach,
        }
    }
}

impl<T, C> Drop for JsGuard<T, C> {
    fn drop(&mut self) {
        // 顺序是硬约束：先摘 handler，`closures` 才随后随字段一起释放。
        (self.detach)(&self.target, &self.closures);
    }
}
