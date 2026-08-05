//! IndexedDB 底层：Web 壳所有结构化持久化的共用入口。
//!
//! 四个 object store，语义不同故不混用：
//! - [`KV_STORE`]（`kv`）—— 少量单例键值（身份迁移兜底、已配对设备快照），out-of-line key。
//! - [`SESSION_STORE`]（`sessions`）—— 传输会话，**一条会话一个 key**（session uuid）。
//! - [`INVITE_STORE`]（`invites`）—— 邀请注册表，一条邀请一个 key（capability 的 sha256 hex）。
//! - [`INBOX_STORE`]（`inbox`）—— 收件箱条目，一条条目一个 key（item uuid）。
//!
//! 后三者分 store 而非塞进 `kv`，是为了各自能一次 [`get_all`] 取回全部记录而不会捞到
//! 无关行（浏览器没有 key 前缀扫描的廉价原语）。
//!
//! 事务窗口：每个操作**自开一个事务、内部不跨 await 边界**——IndexedDB 事务在控制权交回
//! 事件循环且无挂起请求时自动提交，跨 `await` 复用同一事务会拿到 `TransactionInactiveError`。
//! 代价是每次操作重开一次连接，Web 壳的写频率（接收侧 checkpoint 每 10 个 chunk = 2.5 MB
//! 一次）下可忽略。
//!
//! **成批的键走 [`put_many`] / [`delete_many`]**：单键路径「一次操作一个事务」在批量场景下
//! 会退化成 N 个事务、逐个 await（清空 100 条历史 = 100 次往返）。批量版把请求全部排进
//! 同一个事务，只在末尾等一次提交。
//!
//! Worker 里没有 `window`（`indexed_db()` 挂在 Window 上），当前基座只跑主线程；真上 Worker
//! 时这里要换 `WorkerGlobalScope::indexed_db()`。

use std::cell::RefCell;

use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{
    Event, IdbDatabase, IdbObjectStore, IdbOpenDbRequest, IdbRequest, IdbTransaction,
    IdbTransactionMode,
};

use crate::error::{WebError, WebResult};
use crate::js_guard::JsGuard;

const DB_NAME: &str = "swarmdrop-web";
/// v1 = 仅 `kv`；v2 新增 `sessions`（传输会话持久化）；v3 新增 `invites`（邀请注册表落盘）；
/// v4 新增 `inbox`（收件箱条目独立成表，不再是已完成接收会话的投影）。
/// **加 store 必须同时提版本号**，否则 `onupgradeneeded` 不触发、新 store 建不出来。
const DB_VERSION: u32 = 4;

/// 单例键值 store（身份 / 已配对设备）。
pub const KV_STORE: &str = "kv";
/// 传输会话 store（key = session uuid 字符串）。
pub const SESSION_STORE: &str = "sessions";
/// 邀请注册表 store（key = capability 的 sha256 hex）。
pub const INVITE_STORE: &str = "invites";
/// 收件箱 store（key = inbox item uuid 字符串）。
pub const INBOX_STORE: &str = "inbox";

/// 读一个键（不存在 → `None`）。
async fn get(store_name: &str, key: &str) -> WebResult<Option<JsValue>> {
    let db = open().await?;
    let store = object_store(&db, store_name, IdbTransactionMode::Readonly)?;
    let value = request(store.get(&JsValue::from_str(key)).map_err(js_error)?).await?;
    Ok((!value.is_undefined() && !value.is_null()).then_some(value))
}

/// 写一个键（结构化克隆，调用方决定 value 形态）。
async fn put(store_name: &str, key: &str, value: &JsValue) -> WebResult<()> {
    let db = open().await?;
    let store = object_store(&db, store_name, IdbTransactionMode::Readwrite)?;
    request(
        store
            .put_with_key(value, &JsValue::from_str(key))
            .map_err(js_error)?,
    )
    .await?;
    Ok(())
}

/// 删一个键（不存在也算成功，IndexedDB `delete` 本身幂等）。
pub async fn delete(store_name: &str, key: &str) -> WebResult<()> {
    let db = open().await?;
    let store = object_store(&db, store_name, IdbTransactionMode::Readwrite)?;
    request(store.delete(&JsValue::from_str(key)).map_err(js_error)?).await?;
    Ok(())
}

/// 删一批键：**一个事务**排完全部 `delete` 请求，末尾只等一次提交。
///
/// 逐条调 [`delete`] 的代价不是常数——「清空传输历史」最多 100 条终态会话，那就是
/// 100 个独立事务、100 次串行往返。
pub async fn delete_many(store_name: &str, keys: &[String]) -> WebResult<()> {
    if keys.is_empty() {
        return Ok(());
    }
    write_batch(store_name, |store| {
        for key in keys {
            store.delete(&JsValue::from_str(key))?;
        }
        Ok(())
    })
    .await
}

/// 写一批字符串值：**一个事务**排完全部 `put` 请求，末尾只等一次提交。
/// 单键写照旧走 [`put_string`]（接收侧 checkpoint 的热路径，不必绕批量）。
pub async fn put_many(store_name: &str, entries: &[(String, String)]) -> WebResult<()> {
    if entries.is_empty() {
        return Ok(());
    }
    write_batch(store_name, |store| {
        for (key, value) in entries {
            store.put_with_key(&JsValue::from_str(value), &JsValue::from_str(key))?;
        }
        Ok(())
    })
    .await
}

/// 批量写的公共骨架：开一个 readwrite 事务 → 同步排完全部请求 → 等事务提交。
///
/// **`fill` 是同步闭包，这是约束不是风格**：事务在控制权交回事件循环且无挂起请求时即失活，
/// 中途 await 再拿 store 会撞 `TransactionInactiveError`。签名不给它 await 的机会。
async fn write_batch(
    store_name: &str,
    fill: impl FnOnce(&IdbObjectStore) -> Result<(), JsValue>,
) -> WebResult<()> {
    let db = open().await?;
    let tx = db
        .transaction_with_str_and_mode(store_name, IdbTransactionMode::Readwrite)
        .map_err(js_error)?;
    let store = tx.object_store(store_name).map_err(js_error)?;
    if let Err(e) = fill(&store) {
        // 已排进去的请求随 abort 一并回滚——批量写要么整批生效，要么整批不生效，
        // 不留「删了一半」这种没人能推理的中间态。
        let _ = tx.abort();
        return Err(js_error(e));
    }
    transaction_done(tx).await
}

/// `IDBTransaction` 的提交 → future。
///
/// 等的是**事务**而不是逐条请求：批量路径的成功判据是「整批提交」，而逐条 await 会让
/// 事务在两条请求之间失活（[`request`] 的 resume 发生在 success 回调之后的微任务里）。
async fn transaction_done(tx: IdbTransaction) -> WebResult<()> {
    let (sender, rx) = futures::channel::oneshot::channel();
    let slot = std::rc::Rc::new(RefCell::new(Some(sender)));

    let complete_slot = slot.clone();
    let on_complete = Closure::wrap(Box::new(move |_event: Event| {
        if let Some(sender) = complete_slot.borrow_mut().take() {
            let _ = sender.send(Ok(()));
        }
    }) as Box<dyn FnMut(_)>);

    // error 与 abort 共用一个失败出口：前者是请求出错后事务自动中止，后者是显式 abort，
    // 对调用方都是「这批没落地」。
    let failure_tx = tx.clone();
    let on_failure = Closure::wrap(Box::new(move |_event: Event| {
        if let Some(sender) = slot.borrow_mut().take() {
            let error = failure_tx
                .error()
                .map(JsValue::from)
                .unwrap_or_else(|| JsValue::from_str("IndexedDB 事务未提交"));
            let _ = sender.send(Err(error));
        }
    }) as Box<dyn FnMut(_)>);

    tx.set_oncomplete(Some(on_complete.as_ref().unchecked_ref()));
    tx.set_onerror(Some(on_failure.as_ref().unchecked_ref()));
    tx.set_onabort(Some(on_failure.as_ref().unchecked_ref()));
    let _guard = JsGuard::new(tx, [on_complete, on_failure], |tx, _| {
        tx.set_oncomplete(None);
        tx.set_onerror(None);
        tx.set_onabort(None);
    });

    match rx.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(js_error(error)),
        // 三个回调都没触发就被丢弃——理论上不可达（事务必然 settle）。
        Err(_) => Err(WebError::storage("IndexedDB 事务未返回结果")),
    }
}

/// 取回一个 store 的全部值（启动恢复用）。
pub async fn get_all(store_name: &str) -> WebResult<Vec<JsValue>> {
    let db = open().await?;
    let store = object_store(&db, store_name, IdbTransactionMode::Readonly)?;
    let value = request(store.get_all().map_err(js_error)?).await?;
    let array: js_sys::Array = value
        .dyn_into()
        .map_err(|_| WebError::storage("IndexedDB getAll 返回了非数组"))?;
    Ok(array.iter().collect())
}

/// 字符串值的便捷读（`kv` store 的既有形态）。
pub async fn get_string(store_name: &str, key: &str) -> WebResult<Option<String>> {
    Ok(get(store_name, key).await?.and_then(|v| v.as_string()))
}

/// 字符串值的便捷写。
pub async fn put_string(store_name: &str, key: &str, value: &str) -> WebResult<()> {
    put(store_name, key, &JsValue::from_str(value)).await
}

thread_local! {
    /// 进程级单连接。**不缓存的代价不是常数**：接收侧 checkpoint 每 10 个 chunk 落一次盘
    /// （≈ 12 次/秒），每次重开连接既加一次 `indexedDB.open()` 往返到接收热路径上，又多攒
    /// 一条永不关闭的 live connection。wasm 单线程，`RefCell` 足够。
    static CACHED_DB: RefCell<Option<IdbDatabase>> = const { RefCell::new(None) };
}

async fn open() -> WebResult<IdbDatabase> {
    if let Some(db) = CACHED_DB.with(|slot| slot.borrow().clone()) {
        return Ok(db);
    }

    let factory = web_sys::window()
        .ok_or_else(|| WebError::storage("当前环境没有 Window，无法打开 IndexedDB"))?
        .indexed_db()
        .map_err(js_error)?
        .ok_or_else(|| WebError::storage("当前浏览器不支持 IndexedDB"))?;
    let open_request = factory
        .open_with_u32(DB_NAME, DB_VERSION)
        .map_err(js_error)?;
    // 句柄活到 open settle 之后再 drop——`onupgradeneeded` 必在此之前触发。
    let _upgrade_guard = install_upgrade_handler(&open_request);
    let db = request(open_request.unchecked_into::<IdbRequest>())
        .await?
        .dyn_into::<IdbDatabase>()
        .map_err(|_| WebError::storage("打开 IndexedDB 返回了非数据库对象"))?;

    // 并发 open 的去重：`WebTransferStore::load` 会同时发起会话表与收件箱表的读取，两条都
    // 可能在缓存填上之前撞进这里。晚到的那个把自己关掉、复用先到的连接——否则下面那个
    // 「每进程最多一次」的 `forget()` 会随并发度翻倍，且被 forget 的闭包永久钉住第二条
    // live connection（它再也不会被 GC 掉）。
    if let Some(cached) = CACHED_DB.with(|slot| slot.borrow().clone()) {
        db.close();
        return Ok(cached);
    }

    // 常开连接会挡住别的标签页升级版本（`open` 永久 blocked）。收到 versionchange 就让路：
    // 关连接、清缓存，下次操作自然重开到新版本。
    let closing = db.clone();
    let on_version_change = Closure::wrap(Box::new(move |_event: Event| {
        closing.close();
        CACHED_DB.with(|slot| *slot.borrow_mut() = None);
    }) as Box<dyn FnMut(_)>);
    db.set_onversionchange(Some(on_version_change.as_ref().unchecked_ref()));
    // 这一个 forget 是有意的、且有界（每进程最多一次）：处理器要与连接同寿。
    on_version_change.forget();

    CACHED_DB.with(|slot| *slot.borrow_mut() = Some(db.clone()));
    Ok(db)
}

/// 建缺失的 object store。逐个判存在性而非按版本号分支——老库升级与新库首建走同一段代码。
///
/// 返回的 guard 由调用方持有到 open 请求 settle：`forget()` 会永久泄漏 Box 及其捕获的
/// `JsValue`（连带钉住那条连接），而这个函数在写路径上被高频调用。
fn install_upgrade_handler(
    open_request: &IdbOpenDbRequest,
) -> JsGuard<IdbOpenDbRequest, Closure<dyn FnMut(Event)>> {
    let upgrade_request = open_request.clone();
    let on_upgrade = Closure::wrap(Box::new(move |_event: Event| {
        if let Ok(db) = upgrade_request
            .result()
            .and_then(|value| value.dyn_into::<IdbDatabase>())
        {
            for name in [KV_STORE, SESSION_STORE, INVITE_STORE, INBOX_STORE] {
                if !db.object_store_names().contains(name) {
                    let _ = db.create_object_store(name);
                }
            }
        }
    }) as Box<dyn FnMut(_)>);
    open_request.set_onupgradeneeded(Some(on_upgrade.as_ref().unchecked_ref()));
    JsGuard::new(open_request.clone(), on_upgrade, |open_request, _| {
        open_request.set_onupgradeneeded(None);
    })
}

fn object_store(
    db: &IdbDatabase,
    name: &str,
    mode: IdbTransactionMode,
) -> WebResult<IdbObjectStore> {
    db.transaction_with_str_and_mode(name, mode)
        .and_then(|tx| tx.object_store(name))
        .map_err(js_error)
}

/// `IDBRequest` → future（wasm-bindgen 没有现成桥）。
///
/// 回调句柄**持有到请求 settle 后随本函数一起 drop**，不 `forget()`：这条路径每次
/// checkpoint 都走一遍，泄漏两个 `Closure` 就等于泄漏它们捕获的 `IdbRequest`（进而
/// 是那条连接），量正比于传输字节数。请求只 settle 一次，drop 后不会再被调用。
async fn request(request: IdbRequest) -> WebResult<JsValue> {
    let (tx, rx) = futures::channel::oneshot::channel();
    let (tx_ok, tx_err) = {
        let cell = std::rc::Rc::new(RefCell::new(Some(tx)));
        (cell.clone(), cell)
    };

    let success_request = request.clone();
    let on_success = Closure::wrap(Box::new(move |_event: Event| {
        if let Some(tx) = tx_ok.borrow_mut().take() {
            let _ = tx.send(Ok(success_request.result().unwrap_or(JsValue::UNDEFINED)));
        }
    }) as Box<dyn FnMut(_)>);

    let error_request = request.clone();
    let on_error = Closure::wrap(Box::new(move |_event: Event| {
        if let Some(tx) = tx_err.borrow_mut().take() {
            let error = error_request
                .error()
                .ok()
                .flatten()
                .map(JsValue::from)
                .unwrap_or_else(|| JsValue::from_str("IndexedDB 请求失败"));
            let _ = tx.send(Err(error));
        }
    }) as Box<dyn FnMut(_)>);

    request.set_onsuccess(Some(on_success.as_ref().unchecked_ref()));
    request.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    // 正常路径下请求已 settle、不会再有事件，摘不摘都一样；但 future 一旦在 `await` 处被
    // **取消**（外层 select 落到另一分支、超时），请求仍在飞——那时就全靠这里摘干净，
    // 否则事件会打到已释放的闭包上。眼下 Web 壳还没有取消存储操作的路径，这是把不变量
    // 钉住，免得日后加超时时静默踩雷。
    let _guard = JsGuard::new(request, [on_success, on_error], |request, _| {
        request.set_onsuccess(None);
        request.set_onerror(None);
    });

    match rx.await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(js_error(error)),
        // 两个回调都没触发就被丢弃——理论上不可达（IDBRequest 必然 settle）。
        Err(_) => Err(WebError::storage("IndexedDB 请求未返回结果")),
    }
}

fn js_error(value: JsValue) -> WebError {
    WebError::storage(crate::error::js_message(&value, "IndexedDB 操作失败"))
}
