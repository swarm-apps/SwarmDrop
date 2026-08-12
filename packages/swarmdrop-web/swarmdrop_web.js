/* @ts-self-types="./swarmdrop_web.d.ts" */

export class IntoUnderlyingByteSource {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        IntoUnderlyingByteSourceFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_intounderlyingbytesource_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get autoAllocateChunkSize() {
        const ret = wasm.intounderlyingbytesource_autoAllocateChunkSize(this.__wbg_ptr);
        return ret >>> 0;
    }
    cancel() {
        const ptr = this.__destroy_into_raw();
        wasm.intounderlyingbytesource_cancel(ptr);
    }
    /**
     * @param {ReadableByteStreamController} controller
     * @returns {Promise<any>}
     */
    pull(controller) {
        const ret = wasm.intounderlyingbytesource_pull(this.__wbg_ptr, controller);
        return ret;
    }
    /**
     * @param {ReadableByteStreamController} controller
     */
    start(controller) {
        wasm.intounderlyingbytesource_start(this.__wbg_ptr, controller);
    }
    /**
     * @returns {ReadableStreamType}
     */
    get type() {
        const ret = wasm.intounderlyingbytesource_type(this.__wbg_ptr);
        return __wbindgen_enum_ReadableStreamType[ret];
    }
}
if (Symbol.dispose) IntoUnderlyingByteSource.prototype[Symbol.dispose] = IntoUnderlyingByteSource.prototype.free;

export class IntoUnderlyingSink {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        IntoUnderlyingSinkFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_intounderlyingsink_free(ptr, 0);
    }
    /**
     * @param {any} reason
     * @returns {Promise<any>}
     */
    abort(reason) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.intounderlyingsink_abort(ptr, reason);
        return ret;
    }
    /**
     * @returns {Promise<any>}
     */
    close() {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.intounderlyingsink_close(ptr);
        return ret;
    }
    /**
     * @param {any} chunk
     * @returns {Promise<any>}
     */
    write(chunk) {
        const ret = wasm.intounderlyingsink_write(this.__wbg_ptr, chunk);
        return ret;
    }
}
if (Symbol.dispose) IntoUnderlyingSink.prototype[Symbol.dispose] = IntoUnderlyingSink.prototype.free;

export class IntoUnderlyingSource {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(IntoUnderlyingSource.prototype);
        obj.__wbg_ptr = ptr;
        IntoUnderlyingSourceFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        IntoUnderlyingSourceFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_intounderlyingsource_free(ptr, 0);
    }
    cancel() {
        const ptr = this.__destroy_into_raw();
        wasm.intounderlyingsource_cancel(ptr);
    }
    /**
     * @param {ReadableStreamDefaultController} controller
     * @returns {Promise<any>}
     */
    pull(controller) {
        const ret = wasm.intounderlyingsource_pull(this.__wbg_ptr, controller);
        return ret;
    }
}
if (Symbol.dispose) IntoUnderlyingSource.prototype[Symbol.dispose] = IntoUnderlyingSource.prototype.free;

/**
 * 浏览器传输端节点。
 */
export class WebNode {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(WebNode.prototype);
        obj.__wbg_ptr = ptr;
        WebNodeFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebNodeFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_webnode_free(ptr, 0);
    }
    /**
     * 接受入站 offer 并开始接收（落 OPFS）。
     * @param {string} session_id
     * @returns {Promise<void>}
     */
    accept_offer(session_id) {
        const ptr0 = passStringToWasm0(session_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_accept_offer(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 归档 / 取消归档收件箱条目。条目不存在时静默成功。
     * @param {string} item_id
     * @param {boolean} archived
     * @returns {Promise<void>}
     */
    archive_inbox_item(item_id, archived) {
        const ptr0 = passStringToWasm0(item_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_archive_inbox_item(this.__wbg_ptr, ptr0, len0, archived);
        return ret;
    }
    /**
     * 取消一条**接收**会话。
     *
     * 与 [`cancel_send`](Self::cancel_send) 同样只是导出：域层通知对端停发、
     * dispatch `UserCommand::Cancel` 进不可续传终态，并调 `cleanup_part_files()`
     * 逐个走 `FileAccess::cleanup_sink` 清掉本次会话开出来的半成品——在 Web 上那就是
     * [`OpfsFileAccess::cleanup_sink`](crate::file_access)，OPFS 里的截断文件会被真删掉。
     *
     * **方向不自动判**（要发送就调 `cancel_send`）：取消是有副作用的操作（发帧、删文件、
     * 写终态），拿它当探针试方向会把「dispatch 失败」误读成「不是这个方向」。
     * @param {string} session_id
     * @returns {Promise<void>}
     */
    cancel_receive(session_id) {
        const ptr0 = passStringToWasm0(session_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_cancel_receive(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 取消一条**发送**会话。
     *
     * 只是一条 wasm 边界上的线，取消语义整套在域层（`crates/transfer`）里做完：
     * 按 wire 发 Cancel 帧通知对端（对方随即清掉自己的半成品）、dispatch
     * `UserCommand::Cancel` 让协调器把会话写成**不可续传**的终态（`recoverable=false`，
     * 故刷新后不会再冒出「续传」按钮）、并按 `session_id` 索引只动这一条会话。
     * **Web 侧不要再补任何本地取消逻辑**，否则就有了第二条状态机路径。
     *
     * 覆盖「offer 已发出、对方还没接受」这条边界：此时没有 send actor，域层会回落到
     * `outbound_offers`（`flow/send.rs`）——丢弃 prepared、照样 dispatch `Cancel`，
     * 所以「发出去等半天对方不理」也止得住损，调用方无需区分。
     * @param {string} session_id
     * @returns {Promise<void>}
     */
    cancel_send(session_id) {
        const ptr0 = passStringToWasm0(session_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_cancel_send(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 清空传输历史：删除所有**已结束**的会话记录，进行中与已中断的一条不动。
     *
     * 同样只清账本，收件箱里的文件不受影响（见 [`delete_transfer_session`](Self::delete_transfer_session)）。
     * @returns {Promise<void>}
     */
    clear_transfer_history() {
        const ret = wasm.webnode_clear_transfer_history(this.__wbg_ptr);
        return ret;
    }
    /**
     * 关停节点：NetManager::shutdown 取消内部 token（停 presence / infra / event-loop +
     * transfer cleanup，drop Router 停路由）并关 Endpoint（drop Swarm → 断连）——
     * 与 `WebNode.endpoint` 是同一 handle，无需再显式关一次。
     * @returns {Promise<void>}
     */
    close() {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.webnode_close(ptr);
        return ret;
    }
    /**
     * 拨任意 multiaddr（`.../ws` 或 `.../webrtc-direct/certhash/...`，须带 `/p2p/<id>`）。
     * 返回结构化的连接信息（`{ path: "local"|"direct"|"relayed", addr }`）。
     *
     * `signal`（可选）：标准 `AbortSignal`——超时组合用平台原语表达
     * （`AbortSignal.timeout(5000)` / `AbortSignal.any([...])`）。abort 时 Promise
     * 立即以 `{ kind: "aborted" }` reject；**abort ≠ 撤回拨号**（在途拨号继续到
     * 自然失败，无常驻意图残留）。不传 signal 时由内核兜底超时（Browser 15s）
     * 保证有限时间内 settle。
     *
     * ⚠️ **不要拿它判定引导节点或中继的可达性。** 三条理由：它会把候选地址**永久**写进
     * 地址簿且没有失败回滚；对**已连接**的对端它直接返回既有连接快照，于是对已经连上的
     * 内置节点永远返回成功——一个不可能失败的测试比没有测试更坏；而且它测的是直连链路，
     * 中继的实际用法是 reservation，两条链路不同。可达性看
     * [`infra_links`](Self::infra_links) 里那条关系的状态。
     * @param {string} addr
     * @param {AbortSignal | null} [signal]
     * @returns {Promise<ConnectionJson>}
     */
    connect(addr, signal) {
        const ptr0 = passStringToWasm0(addr, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_connect(this.__wbg_ptr, ptr0, len0, isLikeNone(signal) ? 0 : addToExternrefTable0(signal));
        return ret;
    }
    /**
     * 受邀方：消费邀请串完成**真配对握手**。
     *
     * `pair_with_invite` 解码验签 → TTL 预检 → 按 `TransportPolicy` 过滤地址 → 连邀请方出示
     * capability（`PairingMethod::Invite`）→ 邀请方（桌面）校验 CAS 一次性消费 + 用户确认 →
     * 双方写配对记录。身份 pin 由握手强制（连到的必然是 `inviter_id`）。确认发生在**邀请方**
     * 侧，浏览器侧无需交互。配对后该对端进入本机信任表，双向传输（收 / 发）不再被
     * `NotPaired` 拦。
     *
     * 返回 [`PairingOutcomeJson`]：`refused` 非空表示对方拒绝了（**不是错误**），
     * 否则 `peerId` 是已配对对端的 NodeId，`persisted` 为 `false` 时表示配对成功了但没写进
     * IndexedDB —— 刷新页面后这台设备会不见（对端仍记着）。
     * @param {string} invite
     * @returns {Promise<PairingOutcomeJson>}
     */
    connect_invite(invite) {
        const ptr0 = passStringToWasm0(invite, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_connect_invite(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 当前已连接的 **SwarmDrop 客户端**数。
     *
     * 与桌面/移动的 `NetworkStatus.connected_peers` **走同一个函数**
     * （`crates/core/src/network/manager.rs` 也是 `self.devices.connected_count()`），
     * 所以三端这个数的口径天然一致。Web 此前没有这个绑定，设置页只能拿「已配对设备里
     * 在线的台数」凑数——那是 presence 快照，未配对的对端不在里面。
     *
     * **不要改成读 `Endpoint::watch_conns()` 的长度。** 那是原始连接表，
     * `publish_conns` 对每个 `ConnectionEstablished` 都建条目，不区分对端类型；
     * 而浏览器启动时必然会连上至少一条 relay（`ensureConfiguredRelays`，那是公网可达的
     * 前提），于是空载稳态就会显示「已连接 1 · 已配对 0」——一台设备都没配对却说连着一个。
     * `connected_count()` 过滤 `is_swarmdrop_agent`，而 bootstrap/relay 的
     * `agent_version` 是 `swarm-bootstrap/` 前缀（`crates/host/src/device.rs`），
     * 正好被排除。代价是它依赖 identify 完成，比原始连接表晚约一个 RTT——桌面同此。
     *
     * **不一并导出 NAT 状态**：`Endpoint::watch_nat()` 的唯一写入点是 autonat 事件，
     * 而 autonat 是 native-only（见 `crates/net/src/actor.rs` 的 `WatchSenders::nat`，
     * 那里挂着 `cfg_attr(wasm_browser, expect(dead_code))`），wasm 下它恒为 `Unknown`。
     * 导出一个永远不变的常量只是给界面添一行假状态；浏览器版的「别人能不能拨到我」
     * 由 circuit 预留回答，那条已经有了（`infra_links`）。
     * @returns {number}
     */
    connected_peers() {
        const ret = wasm.webnode_connected_peers(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * 解码并验签邀请串，返回对端展示信息 —— **不发起配对、不消费**。
     *
     * 供受邀方在粘贴 / 点链接进来之后先亮一张确认卡：篡改、伪造、格式不认的邀请在这里
     * 就被拒掉，用户点「配对」才走 [`connect_invite`](Self::connect_invite)。
     *
     * **纯本地**：不拨号、不查 DHT、不碰 IndexedDB，全程零出网 —— 确认卡出现之前不该有
     * 任何网络行为，这条是它成立的依据。
     *
     * **判不出「已撤销」**：撤销状态只在邀请方的注册表里，受邀方手上只有一段自包含的
     * 签名串，那件事根本没传播过来。要判就得出网，与上一条冲突。所以撤销只能在
     * `connect_invite` 阶段由邀请方拒绝，调用方把那个失败渲染成人话即可 ——
     * **不要在本地发明撤销判据**（最容易发明的「查 `list_invites` 看在不在」尤其错：
     * 那是本机自己发出的邀请，对受邀方永远为空，于是所有邀请都会被判成已撤销）。
     *
     * 同步返回：与 [`invite_qr_svg`](Self::invite_qr_svg) 一样是纯计算，`&self` 只是
     * 可达性的代价（前端拿模块句柄的唯一路径是 `getNode()`）。
     * @param {string} invite
     * @returns {PairInvitePreviewJson}
     */
    decode_invite_preview(invite) {
        const ptr0 = passStringToWasm0(invite, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_decode_invite_preview(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * 软删除收件箱条目；`delete_local_files` 为真时连 OPFS 里的文件一起删。
     *
     * 与桌面 `delete_inbox_item(item_id, delete_local_files)` 同签名同语义：**是否连文件
     * 一起删由宿主决定，端口只管账本**（`delete_inbox_item_record` 永远只软删记录）。
     *
     * 不删文件时那份 OPFS 副本会成为孤儿——记录一软删，`list`/`search`/`detail` 就都看不到
     * 它了，配额却还占着，用户唯一的出路是浏览器的「清除站点数据」。所以这个入口不是锦上添花：
     * 没有它，Web 端的每一次删除都在泄漏。
     *
     * 编排（顺序、失败处理、幂等）是**三端共用的领域规则**，住在
     * [`swarmdrop_transfer::inbox::delete_inbox_item`]——本方法只做参数解析与错误转换。
     * 「OPFS 的键要剥掉 `opfs:/` 前缀」那一层在 [`OpfsFileAccess::delete_finalized_file`]，
     * 编排不需要知道哪一端用哪个字段。
     * @param {string} item_id
     * @param {boolean} delete_local_files
     * @returns {Promise<void>}
     */
    delete_inbox_item(item_id, delete_local_files) {
        const ptr0 = passStringToWasm0(item_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_delete_inbox_item(this.__wbg_ptr, ptr0, len0, delete_local_files);
        return ret;
    }
    /**
     * 删除一条传输记录。
     *
     * **只删记录**：OPFS 里已落盘的文件不动，收件箱照旧能看能下载——文件的生命周期归
     * 收件箱侧管（三端一致的分工，别在这里发明 Web 特例）。
     *
     * 进行中的会话会被域层拒绝（`TransferManager::delete_session` 的守卫），错误经
     * `WebError` 透出——UI 的按钮可见性只是第一道，绕过它直调导出同样删不掉。
     * @param {string} session_id
     * @returns {Promise<void>}
     */
    delete_transfer_session(session_id) {
        const ptr0 = passStringToWasm0(session_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_delete_transfer_session(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 完成接收后，把 OPFS 里的文件读回成 blob URL 供 `<a download>` 下载。
     * @param {string} relative_path
     * @returns {Promise<string>}
     */
    download_url(relative_path) {
        const ptr0 = passStringToWasm0(relative_path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_download_url(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 传输事件流（逐条产出 `WebTransferEvent` 序列化对象）。**只能取一次**（单点消费）。
     * @returns {ReadableStream<WebTransferEvent>}
     */
    events() {
        const ret = wasm.webnode_events(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * 发起方（browser-as-inviter）：生成一次性签名邀请串，供桌面/移动扫码或粘贴消费。
     *
     * `local_only=true` 走 LocalOnly（受邀方只用私网地址）。邀请自包含本机 dialable 地址提示——
     * 浏览器不 listen 本地 socket，其可达地址来自 **relay reservation**（circuit 地址）；故桌面要
     * 拨得到本机，本机需先经 [`infra_ensure`](Self::infra_ensure) 在某引导节点上建 reservation
     * （等到 `active`），否则邀请里无可拨地址、消费方连不上。
     * **async 化于 invite-persistence**：生成时要把邀请写穿进 IndexedDB，否则刷新页面
     * 后本机就不认识刚发出去的那条邀请了（注册表 fail-closed，查不到即拒绝）。
     * @param {boolean} local_only
     * @returns {Promise<string>}
     */
    generate_invite(local_only) {
        const ret = wasm.webnode_generate_invite(this.__wbg_ptr, local_only);
        return ret;
    }
    /**
     * 单条收件箱详情（含文件清单与关联传输投影）；不存在或已软删返回 `null`。
     * @param {string} item_id
     * @returns {Promise<InboxItemDetail | null>}
     */
    inbox_item(item_id) {
        const ptr0 = passStringToWasm0(item_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_inbox_item(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 按传输会话 id 取收件箱详情（「这次传输收到的东西」的反查）；无关联返回 `null`。
     * @param {string} session_id
     * @returns {Promise<InboxItemDetail | null>}
     */
    inbox_item_by_session(session_id) {
        const ptr0 = passStringToWasm0(session_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_inbox_item_by_session(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 收件箱条目列表，按 `receivedAt` 倒序；`includeArchived=false` 时排除已归档项，
     * 软删项一律不返回。
     *
     * **返回的是完整详情**（含文件清单与关联传输投影），不是 summary。前端此前拿到
     * summary 后要 `Promise.all(summaries.map(inbox_item))` 逐条补详情——1 + N 次 wasm
     * 调用，且拉详情与拉列表之间条目可能已被删（于是要 `filter(d => d !== null)` 去兜
     * 一个自己制造出来的竞态）。而收件箱在浏览器侧是全内存表，列表与详情读的是同一份
     * 数据，那 N 次调用买不到任何新鲜度。
     * @param {boolean} include_archived
     * @returns {Promise<InboxItemDetail[]>}
     */
    inbox_items(include_archived) {
        const ret = wasm.webnode_inbox_items(this.__wbg_ptr, include_archived);
        return ret;
    }
    /**
     * 基础设施状态变化流：每次变化产出一份全量快照（可直接 setState）。
     * 可多次调用（每次独立订阅），与 `events()` 的单点消费不同。
     *
     * **触发源是 `watch_relays`**：内核不外露候选表与在途拨号的变化，而 relay 轨道的
     * 每一次翻转（Connecting / Active / Failed）都从那里出。意图侧的增删由调用方自己
     * 知道（它就是发起方），补一次 `infra_links()` 即可。
     * @returns {ReadableStream<InfraLink[]>}
     */
    infra_changed() {
        const ret = wasm.webnode_infra_changed(this.__wbg_ptr);
        return ret;
    }
    /**
     * 撤销基础设施意图（[`infra_ensure`](Self::infra_ensure) 的对称面）。
     *
     * **真撤销**而非停止等待：停止后台收敛重试、关闭 circuit listener、立刻
     * 断开与该节点的连接（含中止在途拨号），条目从状态集合消失。
     * @param {string} peer_id
     * @returns {Promise<void>}
     */
    infra_drop(peer_id) {
        const ptr0 = passStringToWasm0(peer_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_infra_drop(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 登记一个基础设施节点的常驻意图（校验 + 登记，同步返回）。
     *
     * 浏览器被动接收连接的唯一入口。拨号 / reservation / 断线重建由 core 的
     * InfraSupervisor 统一收敛（最迟 1s 内启动第一轮，失败退避重试）；进度经
     * [`infra_links`](Self::infra_links) / [`infra_changed`](Self::infra_changed)
     * 观测，或用 [`infra_until_active`](Self::infra_until_active) 等首次建立。
     *
     * **校验走 core 的 `add_infra_node`，前端不重写一份规则。** 三条判据里有两条要
     * 认识内核事实（合法 peer id 形状、本端点**实际装配了哪些 transport**），后者正是
     * 浏览器最容易踩的——粘一条 `/tcp/` 进来今天会被静静收下，然后永远连不上且毫无提示。
     * 失败时 reject 一个 `InfraAddrError`（`{ kind, … }`，形状见 bindings.ts），
     * **不是** `WebError`：它要回答的是「这条地址哪里不对」，而不是「哪一层出了错」。
     *
     * **`Duplicate` 也照常 reject。** 它曾被这里吞成成功，理由是「回放要幂等」——不成立：
     * 回放（`replayInfraNodes`）本来就 try/catch 且只 `console.error`，而且它跑在一张空的
     * 候选表上，压根产不出重复。代价却是实打实的：用户粘一条已在清单里的地址会看到
     * 「已添加引导节点，正在连接…」而其实什么都没发生，`duplicate` 那句文案成了死代码。
     * 登记的**效果**仍然幂等（core 的 upsert 会合并），幂等的是状态不是回执。
     *
     * 全部规则零网络往返。「它到底连不连得上」由提交后的收敛环回答——那测的才是后续
     * 真正会走的那条链路（旧的「测试连通性」按钮走直连，对已连上的节点永远绿）。
     *
     * 返回节点的 base58 NodeId——即 `infra_drop` / `infra_until_active` 的入参，
     * 调用方直接串联，无需自行解析 multiaddr 的 `/p2p/` 段。
     * @param {string} addr
     * @returns {string}
     */
    infra_ensure(addr) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(addr, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.webnode_infra_ensure(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * 全量基础设施关系快照（[`InfraLink`](swarmdrop_core::infra::InfraLink)`[]`）。
     *
     * 每条同时带**意图侧**（地址 / 来源 / 角色 / scope / 首末次见到 / 能否移除）与
     * **观测侧**（是否已连、relay 轨道状态与失败原文）。零存储读模型，现场 join
     * 候选表与内核两条 watch——所以「状态粘死」在物理上不可能发生。
     * @returns {InfraLink[]}
     */
    infra_links() {
        const ret = wasm.webnode_infra_links(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * 等待某条关系的 relay 首次进入 `active`，resolve 出 circuit 可达地址（内核拼装）。
     *
     * 观察到 `failed` 时**立即 reject**（把「要不要再等下一轮退避」还给调用方），
     * 意图保留——要停止后台收敛请调 [`infra_drop`](Self::infra_drop)。
     * `signal`（可选）：abort 只是不再等待，同样不改变意图生命周期。
     * 不传 signal 时 30s 兜底超时保证 Promise 有限时间内 settle。
     * @param {string} peer_id
     * @param {AbortSignal | null} [signal]
     * @returns {Promise<string>}
     */
    infra_until_active(peer_id, signal) {
        const ptr0 = passStringToWasm0(peer_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_infra_until_active(this.__wbg_ptr, ptr0, len0, isLikeNone(signal) ? 0 : addToExternrefTable0(signal));
        return ret;
    }
    /**
     * 邀请二维码的 SVG 字符串（深模块 + 透明背景，渲染端自己套白卡）。
     *
     * 编码规范由 [`swarmdrop_invite::qr`] 三端单点固化（原样编码 + 最优分段 + ECL::M +
     * quiet zone）——浏览器**不要**另引 JS 二维码库：三端各画一遍，码面规范就会漂，
     * 而漂了的表现是「某一端生成的码另一端扫不出来」，很难归因。
     *
     * **这是纯函数，`&self` 只是可达性的代价，不代表它是节点能力**——别把这里当作
     * 「纯计算也该挂 `WebNode`」的先例。做成自由函数或 `WebNode` 的静态方法都更贴切，
     * 但前端拿 wasm 模块句柄的唯一路径是 `node-runtime.ts` 里那个**不导出**的
     * `loadModule()`（静态 import 会在 Next 预渲染时挂，故只能动态 import + 记忆化）。
     * 走自由函数就得再开一个 `getModule()` 访问器并自己缓存一份——为一个叶子功能
     * 加这套机器不值，而 `getNode()` 是现成的。
     *
     * 同步返回：纯计算，不碰 IndexedDB 也不碰网络。
     * @param {string} invite
     * @returns {string}
     */
    invite_qr_svg(invite) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(invite, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.webnode_invite_qr_svg(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * 本机未过期的已发出邀请（最近生成的在前）。
     *
     * TTL 24h + 跨刷新存活之后，「我现在有几条邀请在外面飘」需要能看见 ——
     * 这个列表与 [`revoke_invite_by_id`](Self::revoke_invite_by_id) 是那段窗口的控制手段。
     * @returns {InviteListItemJson[]}
     */
    list_invites() {
        const ret = wasm.webnode_list_invites(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * 标记条目最近打开时间（用户点开详情/下载时调）。条目不存在时静默成功。
     * @param {string} item_id
     * @returns {Promise<void>}
     */
    mark_inbox_item_opened(item_id) {
        const ptr0 = passStringToWasm0(item_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_mark_inbox_item_opened(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 本节点身份（base58）。
     * @returns {string}
     */
    node_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webnode_node_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * 打开 OPFS 里的一个文件，返回 `File` 句柄（**不读字节**）。
     *
     * 缩略图管线的取图入口：`createImageBitmap` 只吃 `Blob`，所以这里给的是 `File` 本身
     * 而不是 [`download_url`](Self::download_url) 那样的 blob URL——后者还得 `fetch` 一次
     * 绕回 Blob，多一次拷贝，中间那个 URL 也必须记得 revoke。
     *
     * 非 secure origin 下 OPFS 整个不可用，这里会明确报错（而不是永久 pending），
     * 前端据此降级到类型图标。
     * @param {string} relative_path
     * @returns {Promise<File>}
     */
    open_file(relative_path) {
        const ptr0 = passStringToWasm0(relative_path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_open_file(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 已配对设备清单——与桌面 `list_devices` 同源的 [`DeviceManager::get_devices`] 读模型
     * （含在线状态/连接类型，presence 在 Web 侧同样运作）。
     * @returns {Device[]}
     */
    paired_devices() {
        const ret = wasm.webnode_paired_devices(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * 暂停一条**接收**会话。
     *
     * 与 [`pause_send`](Self::pause_send) 对称，但落盘的半成品**不清理**（那是取消才做的事）
     * ——OPFS 里已写入的部分连同 checkpoint 一起留着，`resume` 从断点续。
     *
     * 接收方向的 suspended 会话 `worth_persisting`，所以它**跨刷新也能续**：重新打开页面后
     * 会话仍在传输列表里，「续传」照常可点。
     *
     * **方向不自动判**，理由同 `cancel_*`：暂停有副作用（停 actor、写状态、通知对端），
     * 拿它当探针试方向会在第一条真失败时顺手对另一个方向也来一遍。
     * @param {string} session_id
     * @returns {Promise<void>}
     */
    pause_receive(session_id) {
        const ptr0 = passStringToWasm0(session_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_pause_receive(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 暂停一条**发送**会话。
     *
     * 与取消同样只是一条 wasm 边界上的线：域层停掉 sender actor、把文件级进度落库、
     * dispatch `UserCommand::Pause`（`active` → `suspended(LocalPaused)`，
     * **`recoverable = true`**），并通知对端。之后调 [`resume`](Self::resume) 接着传。
     *
     * ## 浏览器上它为什么恢复得了（与「发送不跨刷新」不矛盾）
     *
     * [`initiate_resume`] 要的两样东西在**同一个页面生命周期内**都还在：
     *
     * - **会话记录**：`WebTransferStore` 是「内存读缓存 + IndexedDB 写穿」，`create_session`
     *   无条件写内存，`worth_persisting` 只决定要不要**再**写 IndexedDB。所以非终态发送
     *   会话查得到，只是刷新后就没了。
     * - **文件内容**：用户选的 `File` 存在 [`OpfsFileAccess`](crate::file_access) 的源注册表
     *   里，登记后不移除，`read_source_chunk` 照常读得到。
     *
     * 刷新之后两样同时消失，`initiate_resume` 在 `find_session` 那一步就报「会话不存在」
     * ——那正是应有的行为，不需要在这里另设守卫（见 `store.rs` 的落库范围表）。
     * @param {string} session_id
     * @returns {Promise<void>}
     */
    pause_send(session_id) {
        const ptr0 = passStringToWasm0(session_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_pause_send(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 当前挂起（待确认）的入站 offer 列表。
     * @returns {OfferJson[]}
     */
    pending_offers() {
        const ret = wasm.webnode_pending_offers(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * 挂起的入站配对请求（消费方扫/粘本机 invite 后到达）。**取出即清空**，调用方自行累积展示。
     * @returns {PendingPairingJson[]}
     */
    pending_pairing_requests() {
        const ret = wasm.webnode_pending_pairing_requests(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * 拒绝入站 offer。
     * @param {string} session_id
     * @returns {Promise<void>}
     */
    reject_offer(session_id) {
        const ptr0 = passStringToWasm0(session_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_reject_offer(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 解除与某台已配对设备的配对（`peer_id` 为 base58 NodeId）。
     *
     * 走 core 的 `PairingManager::unpair`：**先落盘、再删共享内存表、最后发事件**。
     * 持久化失败即整体报错且内存表不动——绝不出现「这次点了就没了、刷新一下又回来」。
     * 删内存表这一步同时撤销 presence 保活与 `is_paired` 判定（一个 tick 内收敛），
     * 所以本方法之后不需要再补任何本地清理。
     *
     * **单方语义**：只解除本机这一侧，对端仍然认得本机；对端再发起传输会被 `NotPaired`
     * 拒掉，要恢复得重新走一次完整配对。
     *
     * 幂等：本来就没配对的 peer 直接返回成功，不发事件。
     * @param {string} peer_id
     * @returns {Promise<void>}
     */
    remove_paired_device(peer_id) {
        const ptr0 = passStringToWasm0(peer_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_remove_paired_device(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 改本机设备名：落盘 → 本机 `OsInfo` → identify 的 `agent_version` → 发
     * `DeviceRenamed`（编排在 core 的 `device_name::rename_device`，三端同一份）。
     *
     * **已连接的对端一个 RTT 内就看到新名字**：新值逐连接下发给每条已建立连接的
     * identify handler，再向这些对端主动 push；未连接的对端下次连上时直接取到新值。
     * 节点不重启、连接不断、传输不中断，页面也不必刷新。
     *
     * 返回归一化后的名字（`undefined` = 已清空，对外回落到
     * [`default_device_name`](crate::device_config::default_device_name)）。入参经
     * `DeviceName::parse` 归一化（trim、剥控制字符与 `;`、截断到 40 个 char），所以返回值
     * 可能与传进来的不同——UI 要展示的是这个返回值，而不是用户的草稿。
     *
     * 与模块级 [`set_device_name`](crate::device_config::set_device_name) 的分工：那个只
     * 落盘，供节点起不来时的设置页用；节点在跑就走这里。两者的分支在 JS 侧
     * （`node-runtime.ts`）——节点句柄只活在那边，Rust 够不到。
     * @param {string | null} [name]
     * @returns {Promise<string | undefined>}
     */
    rename_device(name) {
        var ptr0 = isLikeNone(name) ? 0 : passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_rename_device(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 响应一个入站配对请求（`accept=true` 接受并写配对记录、CAS 消费 invite / `false` 拒绝）。
     * @param {string} pending_id
     * @param {boolean} accept
     * @returns {Promise<boolean>}
     */
    respond_pairing_request(pending_id, accept) {
        const ptr0 = passStringToWasm0(pending_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_respond_pairing_request(this.__wbg_ptr, ptr0, len0, accept);
        return ret;
    }
    /**
     * 手动发起断点续传（对某 suspended 会话）。
     *
     * 三种 suspended 都走这一条：用户自己暂停的（`LocalPaused`）、对端暂停的
     * （`RemotePaused`）、以及连接中断 / 对方离线。恢复需要对端在线并应答探测，
     * 失败时错误照常经 `WebError` 透出。
     * @param {string} session_id
     * @returns {Promise<void>}
     */
    resume(session_id) {
        const ptr0 = passStringToWasm0(session_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_resume(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 撤销本机发出的邀请（重新生成覆盖旧串、用户放弃、关闭邀请界面）。
     *
     * 幂等且不报错——不认识的串直接 no-op（详见 `PairingManager::revoke_invite`），
     * 调用方 fire-and-forget 即可（**返回 Promise，不 await 也能用**）。
     *
     * async 化于 invite-persistence：撤销要把那行从 IndexedDB 删掉，否则刷新后它又回来了。
     * 返回**是否已落盘**：`false` 时重启后那条邀请会复活，调用方应当提示用户。
     * @param {string} invite
     * @returns {Promise<boolean>}
     */
    revoke_invite(invite) {
        const ptr0 = passStringToWasm0(invite, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_revoke_invite(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 按列表条目的 `id`（capability 哈希 hex）撤销 —— 列表里没有原始邀请串。
     * 返回**是否已落盘**：`false` 时刷新后那条邀请会复活，调用方应当提示用户。
     * @param {string} id
     * @returns {Promise<boolean>}
     */
    revoke_invite_by_id(id) {
        const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_revoke_invite_by_id(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 收件箱子串检索：大小写不敏感，覆盖标题 / 来源设备名 / 文件名与相对路径。
     * 空查询返回空列表；结果按 `receivedAt` 倒序并截断到 `limit`
     * （缺省取三端共享的 `INBOX_SEARCH_LIMIT`，前端不必自带魔数）。
     * @param {string} query
     * @param {number | null | undefined} limit
     * @param {boolean} include_archived
     * @returns {Promise<InboxSearchHit[]>}
     */
    search_inbox(query, limit, include_archived) {
        const ptr0 = passStringToWasm0(query, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_search_inbox(this.__wbg_ptr, ptr0, len0, isLikeNone(limit) ? 0x100000001 : (limit) >>> 0, include_archived);
        return ret;
    }
    /**
     * 向 `to`（base58 NodeId）发送用户选择的文件：登记文件源 → prepare（checksum + bao
     * outboard）→ 发 Offer。返回 session_id。
     * @param {string} to
     * @param {File[]} files
     * @returns {Promise<string>}
     */
    send_files(to, files) {
        const ptr0 = passStringToWasm0(to, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(files, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_send_files(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return ret;
    }
    /**
     * 转发已接收的文件：把 OPFS 里的条目取回成 `File`，之后与用户选文件发送**完全同路**。
     *
     * `paths` 是收件箱条目的 OPFS 相对路径（落盘时写的那个）。`FileSystemFileHandle::get_file()`
     * 返回的正是 `send_files` 已经在吃的 `web_sys::File`，所以读分块那条路径一行都不用动——
     * 转发在后端从来不缺能力，缺的只是一个入口。
     *
     * 拿到的 `File.name()` 是路径末段，`webkitRelativePath` 为空，于是 `relative_path` 回落
     * 到文件名。这是要的行为：转发是一次新的发送，把上一次传输的目录结构带给第三台设备
     * 只会让对方莫名其妙（移动端同此约定）。
     * **取不到的条目被跳过，而不是让整批失败。** OPFS 是配额存储，条目可能被浏览器驱逐；
     * 「一个死路径 → 整次转发失败 → 用户看到一条没有文件名的 DOMException」正是
     * Received File Reuse Contract 里「发起前筛掉」要杜绝的。移动端由 `selectForwardable`
     * 承担这件事，浏览器这边没有对应的 per-path 原语可用，所以筛在这里。
     *
     * 被跳过的路径经 [`Self::take_skipped_forward_paths`] 取回，由 UI 告诉用户。全部取不到
     * 才算失败——那时确实没有任何东西可发。
     * @param {string} to
     * @param {string[]} paths
     * @returns {Promise<string>}
     */
    send_inbox_files(to, paths) {
        const ptr0 = passStringToWasm0(to, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(paths, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_send_inbox_files(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return ret;
    }
    /**
     * 建节点：持久化身份（Window=localStorage / Worker=OPFS）+ IndexedDB 恢复已配对设备 → 包 core 组合根 [`start_node`]
     * （Browser [`EndpointProfile`] + Web 端口）→ 完整 [`NetManager`] + 3 协议 Router（含
     * pairing）。**须在主线程 Window 跑**——webrtc-websys dial 碰 window，Worker 里会 panic。
     * @returns {Promise<WebNode>}
     */
    static spawn() {
        const ret = wasm.webnode_spawn();
        return ret;
    }
    /**
     * 取回上一次转发中被跳过的路径，**取过即清**。
     *
     * 单独一个方法而不是塞进 `send_inbox_files` 的返回值：那个返回的是 session_id，
     * 换成结构体会让所有既有调用点跟着改，而这条信息只有转发这一个入口关心。
     * @returns {string[]}
     */
    take_skipped_forward_paths() {
        const ret = wasm.webnode_take_skipped_forward_paths(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * 已持久化的传输会话投影，**按 `startedAt` 倒序**（端口契约，三端一致）。
     *
     * 页面刷新后事件流从零开始，前端据此回补收件箱与传输活动视图（收件箱 = 其中
     * `direction=receive` 且 `terminalReason=completed` 的条目，文件仍在 OPFS，可继续
     * [`download_url`](Self::download_url)）。各面板再按自己的维度（结束时间 / 更新时间）
     * 重排是预期行为——端口保证的是确定性，不是最终展示序。
     *
     * **不含**非终态的发送会话与待决 offer：浏览器刷新后无法在不重新选择文件的前提下
     * 读回 `File`，待决 offer 也已无处应答，故它们本就不落库（见 `store.rs` 模块注释）。
     * @returns {Promise<TransferProjection[]>}
     */
    transfer_history() {
        const ret = wasm.webnode_transfer_history(this.__wbg_ptr);
        return ret;
    }
    /**
     * 更新已配对设备的信任级别与收件策略。
     *
     * 与桌面 `update_paired_device_policy` 命令**同一条路径**：落盘与「节点在跑时把新值推进
     * 共享内存表」都在 core 的
     * [`set_receive_policy`](swarmdrop_core::paired_devices::set_receive_policy)。
     * 后半句不能省——`swarmdrop_transfer::policy` 裁决入站 offer 时读的是内存表那份，
     * 只落盘会变成「策略已保存、本次运行仍按旧策略放行」。存在性检查也只在那一处。
     *
     * `receive_policy` 传 `undefined` 表示**按新信任级别取默认策略**（`for_trust_level`），
     * 这是「只改信任级别、策略跟着走」那条路径；传具体值则逐字段采用。
     *
     * **返回 `()`，调用方自己重取一次 `paired_devices()`**——与
     * [`remove_paired_device`](Self::remove_paired_device) 同一个约定。两个理由：
     * core 这条路径不发事件（没有对应的 `CoreEvent` 变体，补一条会波及三端全部 event
     * adapter 的穷尽 match，是独立增量）；而 `paired_devices()` 在 Web 侧是同步的内存查询，
     * 重取一次比把 `PairedDeviceInfo`（存储型）也搬进 Web 的类型面便宜——那一面目前只有
     * `Device` 这一个读模型，多一个就多一处要解释「这两个有什么区别」。
     * @param {string} peer_id
     * @param {DeviceTrustLevel} trust_level
     * @param {DeviceReceivePolicy | null} [receive_policy]
     * @returns {Promise<void>}
     */
    update_paired_device_policy(peer_id, trust_level, receive_policy) {
        const ptr0 = passStringToWasm0(peer_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_update_paired_device_policy(this.__wbg_ptr, ptr0, len0, trust_level, isLikeNone(receive_policy) ? 0 : addToExternrefTable0(receive_policy));
        return ret;
    }
}
if (Symbol.dispose) WebNode.prototype[Symbol.dispose] = WebNode.prototype.free;

/**
 * 未设设备名时对外展示的默认值（UA 派生的浏览器名，如 `"Chrome"`）。
 *
 * 导出它而不是让前端再解析一次 UA：两份判定表迟早漂成「设置页 placeholder 写 Safari、
 * 对端看到 Browser」，既难发现又完全没有价值。这里返回的就是 `OsInfo::display_name()`
 * 在 `name` 缺省时回退到的那个 `hostname`。
 * @returns {string}
 */
export function default_device_name() {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.default_device_name();
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}

/**
 * 检索条数上限（[`INBOX_SEARCH_LIMIT`](swarmdrop_transfer::inbox::INBOX_SEARCH_LIMIT) 的只读镜像）。
 *
 * `search_inbox` 的 `limit` 缺省就取这个值、传大了也会被钳回来，前端**不需要**传它。
 * 导出它只为一件事：UI 要说「只显示了最近 N 条」时得知道 N 是几。
 *
 * 换句话说前端仍然不许自带这个数字——那正是 #111 修掉的分叉（此前四个宿主四个值，
 * 而截断掉的永远是最早收到的那批）。wasm-bindgen 不导出常量，所以包成函数。
 * 某信任级别的默认接收策略。
 *
 * **纯派生，不碰节点**，所以是自由函数不是 `WebNode` 方法——它在节点还没起来时也该能用
 * （信任策略对话框可以先开着）。
 *
 * 存在的全部理由是**不让 JS 再抄一份那张表**。桌面与移动此前各抄了一份，两份还长出了不同的
 * 「切级别时保留哪些字段」规则，而内核那一份一个都不保留——同一个产品动作三种行为。
 * 现在规则只在 [`DeviceReceivePolicy::for_trust_level`] 一处，三端各经自己的 binding 调它。
 *
 * `previous` 传该设备**当前**的策略，用户显式设过的保存位置与代收授权会被带过去
 * （`blocked` 除外）。新配对或不关心时传 `undefined`。
 * @param {DeviceTrustLevel} trust_level
 * @param {DeviceReceivePolicy | null} [previous]
 * @returns {DeviceReceivePolicy}
 */
export function default_receive_policy(trust_level, previous) {
    const ret = wasm.default_receive_policy(trust_level, isLikeNone(previous) ? 0 : addToExternrefTable0(previous));
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * 当前持久化的设备名；未设过（或读失败 / 内容非法）返回 `undefined`。
 * @returns {Promise<string | undefined>}
 */
export function get_device_name() {
    const ret = wasm.get_device_name();
    return ret;
}

/**
 * @returns {number}
 */
export function inbox_search_limit() {
    const ret = wasm.inbox_search_limit();
    return ret >>> 0;
}

/**
 * 设置设备名；`null` / 空串 / 归一化后为空一律视为清空，回落到
 * [`default_device_name`]。入参经 `DeviceName::parse` 归一化（trim、剥控制字符与 `;`、
 * 截断到 40 个 char），UI 侧的 `maxLength` 只是提前拦一道。
 * @param {string | null} [name]
 * @returns {Promise<void>}
 */
export function set_device_name(name) {
    var ptr0 = isLikeNone(name) ? 0 : passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    var len0 = WASM_VECTOR_LEN;
    const ret = wasm.set_device_name(ptr0, len0);
    return ret;
}

/**
 * wasm 模块加载即初始化 panic hook + tracing（浏览器 console）。
 */
export function start() {
    wasm.start();
}

function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_8c4e43fe74559d73: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_Number_04624de7d0e8332d: function(arg0) {
            const ret = Number(arg0);
            return ret;
        },
        __wbg_String_8f0eb39a4a4c2f66: function(arg0, arg1) {
            const ret = String(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_bigint_get_as_i64_8fcf4ce7f1ca72a2: function(arg0, arg1) {
            const v = arg1;
            const ret = typeof(v) === 'bigint' ? v : undefined;
            getDataViewMemory0().setBigInt64(arg0 + 8 * 1, isLikeNone(ret) ? BigInt(0) : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_boolean_get_bbbb1c18aa2f5e25: function(arg0) {
            const v = arg0;
            const ret = typeof(v) === 'boolean' ? v : undefined;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg___wbindgen_debug_string_0bc8482c6e3508ae: function(arg0, arg1) {
            const ret = debugString(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_in_47fa6863be6f2f25: function(arg0, arg1) {
            const ret = arg0 in arg1;
            return ret;
        },
        __wbg___wbindgen_is_bigint_31b12575b56f32fc: function(arg0) {
            const ret = typeof(arg0) === 'bigint';
            return ret;
        },
        __wbg___wbindgen_is_function_0095a73b8b156f76: function(arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_null_ac34f5003991759a: function(arg0) {
            const ret = arg0 === null;
            return ret;
        },
        __wbg___wbindgen_is_object_5ae8e5880f2c1fbd: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_cd444516edc5b180: function(arg0) {
            const ret = typeof(arg0) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_9e4d92534c42d778: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_jsval_eq_11888390b0186270: function(arg0, arg1) {
            const ret = arg0 === arg1;
            return ret;
        },
        __wbg___wbindgen_jsval_loose_eq_9dd77d8cd6671811: function(arg0, arg1) {
            const ret = arg0 == arg1;
            return ret;
        },
        __wbg___wbindgen_number_get_8ff4255516ccad3e: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_string_get_72fb696202c56729: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_be289d5034ed271b: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg__wbg_cb_unref_d9b87ff7982e3b21: function(arg0) {
            arg0._wbg_cb_unref();
        },
        __wbg_abort_5151361027dc87df: function() { return handleError(function (arg0) {
            arg0.abort();
        }, arguments); },
        __wbg_abort_8e19e4e93d87a18d: function(arg0) {
            const ret = arg0.abort();
            return ret;
        },
        __wbg_aborted_0b67c37a14dbbc89: function(arg0) {
            const ret = arg0.aborted;
            return ret;
        },
        __wbg_addEventListener_3acb0aad4483804c: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3);
        }, arguments); },
        __wbg_addIceCandidate_123d7292fa766eff: function(arg0, arg1) {
            const ret = arg0.addIceCandidate(arg1);
            return ret;
        },
        __wbg_arrayBuffer_05ce1af23e9064e8: function(arg0) {
            const ret = arg0.arrayBuffer();
            return ret;
        },
        __wbg_buffer_26d0910f3a5bc899: function(arg0) {
            const ret = arg0.buffer;
            return ret;
        },
        __wbg_bufferedAmount_3f2f1736b13827b6: function(arg0) {
            const ret = arg0.bufferedAmount;
            return ret;
        },
        __wbg_byobRequest_80e594e6da4e1af7: function(arg0) {
            const ret = arg0.byobRequest;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_byteLength_3417f266f4bf562a: function(arg0) {
            const ret = arg0.byteLength;
            return ret;
        },
        __wbg_byteOffset_f88547ca47c86358: function(arg0) {
            const ret = arg0.byteOffset;
            return ret;
        },
        __wbg_call_389efe28435a9388: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.call(arg1);
            return ret;
        }, arguments); },
        __wbg_call_4708e0c13bdc8e95: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_cancel_2c0a0a251ff6b2b7: function(arg0) {
            const ret = arg0.cancel();
            return ret;
        },
        __wbg_candidate_e034be3d85919c5f: function(arg0) {
            const ret = arg0.candidate;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_catch_c1f8c7623b458214: function(arg0, arg1) {
            const ret = arg0.catch(arg1);
            return ret;
        },
        __wbg_channel_82b58a29dba55e8a: function(arg0) {
            const ret = arg0.channel;
            return ret;
        },
        __wbg_clearTimeout_5e42188b495715bb: function() { return handleError(function (arg0, arg1) {
            arg0.clearTimeout(arg1);
        }, arguments); },
        __wbg_clearTimeout_96804de0ab838f26: function(arg0) {
            const ret = clearTimeout(arg0);
            return ret;
        },
        __wbg_close_023f23e40c08de17: function(arg0) {
            const ret = arg0.close();
            return ret;
        },
        __wbg_close_06dfa0a815b9d71f: function() { return handleError(function (arg0) {
            arg0.close();
        }, arguments); },
        __wbg_close_47e2271217957c7e: function(arg0) {
            arg0.close();
        },
        __wbg_close_53683f4809368fc7: function(arg0) {
            arg0.close();
        },
        __wbg_close_83fb809aca3de7f9: function(arg0) {
            const ret = arg0.close();
            return ret;
        },
        __wbg_close_a79afee31de55b36: function() { return handleError(function (arg0) {
            arg0.close();
        }, arguments); },
        __wbg_close_f9ba12c30bbb456f: function(arg0) {
            arg0.close();
        },
        __wbg_closed_9020de43877af289: function(arg0) {
            const ret = arg0.closed;
            return ret;
        },
        __wbg_closed_f3dc59c66d3664a7: function(arg0) {
            const ret = arg0.closed;
            return ret;
        },
        __wbg_connectionState_552a7ef94243f9da: function(arg0) {
            const ret = arg0.connectionState;
            return (__wbindgen_enum_RtcPeerConnectionState.indexOf(ret) + 1 || 7) - 1;
        },
        __wbg_contains_bde74fed714d6521: function(arg0, arg1, arg2) {
            const ret = arg0.contains(getStringFromWasm0(arg1, arg2));
            return ret;
        },
        __wbg_createAnswer_a81a236697720f26: function(arg0) {
            const ret = arg0.createAnswer();
            return ret;
        },
        __wbg_createBidirectionalStream_48118f75a605ab18: function(arg0) {
            const ret = arg0.createBidirectionalStream();
            return ret;
        },
        __wbg_createDataChannel_1175bbde394c8293: function(arg0, arg1, arg2, arg3) {
            const ret = arg0.createDataChannel(getStringFromWasm0(arg1, arg2), arg3);
            return ret;
        },
        __wbg_createDataChannel_5b6887f64b34cde3: function(arg0, arg1, arg2) {
            const ret = arg0.createDataChannel(getStringFromWasm0(arg1, arg2));
            return ret;
        },
        __wbg_createObjectStore_545ee23ffd61e3fc: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.createObjectStore(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_createObjectURL_918185db6a10a0c8: function() { return handleError(function (arg0, arg1) {
            const ret = URL.createObjectURL(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_createOffer_ad84508938485425: function(arg0) {
            const ret = arg0.createOffer();
            return ret;
        },
        __wbg_createWritable_6c6623bddc203fe6: function(arg0, arg1) {
            const ret = arg0.createWritable(arg1);
            return ret;
        },
        __wbg_crypto_86f2631e91b51511: function(arg0) {
            const ret = arg0.crypto;
            return ret;
        },
        __wbg_data_5330da50312d0bc1: function(arg0) {
            const ret = arg0.data;
            return ret;
        },
        __wbg_debug_55137df391ebfd29: function(arg0, arg1) {
            var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
            wasm.__wbindgen_free(arg0, arg1 * 4, 4);
            console.debug(...v0);
        },
        __wbg_deleteObjectStore_6f911570c372b5f6: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.deleteObjectStore(getStringFromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_delete_d6d7f750bd9ed2cd: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.delete(arg1);
            return ret;
        }, arguments); },
        __wbg_desiredSize_cd0f9f8beba4c989: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.desiredSize;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        }, arguments); },
        __wbg_enqueue_2c63f2044f257c3e: function() { return handleError(function (arg0, arg1) {
            arg0.enqueue(arg1);
        }, arguments); },
        __wbg_entries_58c7934c745daac7: function(arg0) {
            const ret = Object.entries(arg0);
            return ret;
        },
        __wbg_error_6afb95c784775817: function() { return handleError(function (arg0) {
            const ret = arg0.error;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_error_7534b8e9a36f1ab4: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        },
        __wbg_error_91947ba14c44e1c9: function(arg0, arg1) {
            var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
            wasm.__wbindgen_free(arg0, arg1 * 4, 4);
            console.error(...v0);
        },
        __wbg_error_bf9fa99d609a0ce7: function(arg0) {
            const ret = arg0.error;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_generateCertificate_451abc23dcbd6480: function() { return handleError(function (arg0) {
            const ret = RTCPeerConnection.generateCertificate(arg0);
            return ret;
        }, arguments); },
        __wbg_getAll_33c9f4f22da09509: function() { return handleError(function (arg0) {
            const ret = arg0.getAll();
            return ret;
        }, arguments); },
        __wbg_getDirectoryHandle_87ce8ca53cf4d8dc: function(arg0, arg1, arg2, arg3) {
            const ret = arg0.getDirectoryHandle(getStringFromWasm0(arg1, arg2), arg3);
            return ret;
        },
        __wbg_getDirectory_b66ae3e79f902982: function(arg0) {
            const ret = arg0.getDirectory();
            return ret;
        },
        __wbg_getFileHandle_ff4ab917b45affb3: function(arg0, arg1, arg2, arg3) {
            const ret = arg0.getFileHandle(getStringFromWasm0(arg1, arg2), arg3);
            return ret;
        },
        __wbg_getFile_115354fc950edc88: function(arg0) {
            const ret = arg0.getFile();
            return ret;
        },
        __wbg_getItem_0c792d344808dcf5: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg1.getItem(getStringFromWasm0(arg2, arg3));
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_getRandomValues_4d6521d092b50cf5: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_getRandomValues_a8ddca022803a145: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_getRandomValues_b3f15fcbfabb0f8b: function() { return handleError(function (arg0, arg1) {
            arg0.getRandomValues(arg1);
        }, arguments); },
        __wbg_getReader_804829cfb24eb4dd: function(arg0) {
            const ret = arg0.getReader();
            return ret;
        },
        __wbg_getTime_1e3cd1391c5c3995: function(arg0) {
            const ret = arg0.getTime();
            return ret;
        },
        __wbg_getWriter_4bd085da387cdc1a: function() { return handleError(function (arg0) {
            const ret = arg0.getWriter();
            return ret;
        }, arguments); },
        __wbg_get_5e856edb32ac1289: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.get(arg1);
            return ret;
        }, arguments); },
        __wbg_get_9b94d73e6221f75c: function(arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        },
        __wbg_get_b3ed3ad4be2bc8ac: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_get_with_ref_key_1dc361bd10053bfe: function(arg0, arg1) {
            const ret = arg0[arg1];
            return ret;
        },
        __wbg_has_d4e53238966c12b6: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.has(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_id_5a5e3288567f6f1f: function(arg0) {
            const ret = arg0.id;
            return isLikeNone(ret) ? 0xFFFFFF : ret;
        },
        __wbg_incomingBidirectionalStreams_01c80c459a7f4dfa: function(arg0) {
            const ret = arg0.incomingBidirectionalStreams;
            return ret;
        },
        __wbg_indexedDB_782f0610ea9fb144: function() { return handleError(function (arg0) {
            const ret = arg0.indexedDB;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_instanceof_ArrayBuffer_c367199e2fa2aa04: function(arg0) {
            let result;
            try {
                result = arg0 instanceof ArrayBuffer;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_DomException_99c177193e554b75: function(arg0) {
            let result;
            try {
                result = arg0 instanceof DOMException;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Error_8573fe0b0b480f46: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Error;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_FileSystemDirectoryHandle_56a167039d614548: function(arg0) {
            let result;
            try {
                result = arg0 instanceof FileSystemDirectoryHandle;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_FileSystemFileHandle_fd8948f4bac4e78a: function(arg0) {
            let result;
            try {
                result = arg0 instanceof FileSystemFileHandle;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_FileSystemWritableFileStream_64902a696195f333: function(arg0) {
            let result;
            try {
                result = arg0 instanceof FileSystemWritableFileStream;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_File_21240124aa87092d: function(arg0) {
            let result;
            try {
                result = arg0 instanceof File;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_IdbDatabase_8d723b3ff4761c2d: function(arg0) {
            let result;
            try {
                result = arg0 instanceof IDBDatabase;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_IdbVersionChangeEvent_89f37f7349835a31: function(arg0) {
            let result;
            try {
                result = arg0 instanceof IDBVersionChangeEvent;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_ReadableStreamDefaultReader_8c3866331ce32722: function(arg0) {
            let result;
            try {
                result = arg0 instanceof ReadableStreamDefaultReader;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Uint8Array_9b9075935c74707c: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_WebTransportBidirectionalStream_abe1536df7828016: function(arg0) {
            let result;
            try {
                result = arg0 instanceof WebTransportBidirectionalStream;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Window_ed49b2db8df90359: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Window;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_WorkerGlobalScope_07b9d5514ff0156e: function(arg0) {
            let result;
            try {
                result = arg0 instanceof WorkerGlobalScope;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_isArray_d314bb98fcf08331: function(arg0) {
            const ret = Array.isArray(arg0);
            return ret;
        },
        __wbg_isSafeInteger_bfbc7332a9768d2a: function(arg0) {
            const ret = Number.isSafeInteger(arg0);
            return ret;
        },
        __wbg_isSecureContext_196d8462fd333d8d: function(arg0) {
            const ret = arg0.isSecureContext;
            return ret;
        },
        __wbg_isSecureContext_1e186b850f07cfb3: function(arg0) {
            const ret = arg0.isSecureContext;
            return ret;
        },
        __wbg_label_d02400a47f313907: function(arg0, arg1) {
            const ret = arg1.label;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_lastModified_a5cfce993c651681: function(arg0) {
            const ret = arg0.lastModified;
            return ret;
        },
        __wbg_length_32ed9a279acd054c: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_35a7bace40f36eac: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_localDescription_d1502c826999ccd4: function(arg0) {
            const ret = arg0.localDescription;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_localStorage_a22d31b9eacc4594: function() { return handleError(function (arg0) {
            const ret = arg0.localStorage;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_log_e51ef223c244b133: function(arg0, arg1) {
            var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
            wasm.__wbindgen_free(arg0, arg1 * 4, 4);
            console.log(...v0);
        },
        __wbg_message_0b2b0298a231b0d4: function(arg0, arg1) {
            const ret = arg1.message;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_msCrypto_d562bbe83e0d4b91: function(arg0) {
            const ret = arg0.msCrypto;
            return ret;
        },
        __wbg_name_171cddfde96a29c8: function(arg0, arg1) {
            const ret = arg1.name;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_name_242753e5110cd756: function(arg0, arg1) {
            const ret = arg1.name;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_navigator_43be698ba96fc088: function(arg0) {
            const ret = arg0.navigator;
            return ret;
        },
        __wbg_navigator_4478931f32ebca57: function(arg0) {
            const ret = arg0.navigator;
            return ret;
        },
        __wbg_new_0_73afc35eb544e539: function() {
            const ret = new Date();
            return ret;
        },
        __wbg_new_28132f467c93cf40: function() { return handleError(function (arg0, arg1) {
            const ret = new WebTransport(getStringFromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg_new_361308b2356cecd0: function() {
            const ret = new Object();
            return ret;
        },
        __wbg_new_3eb36ae241fe6f44: function() {
            const ret = new Array();
            return ret;
        },
        __wbg_new_72b49615380db768: function(arg0, arg1) {
            const ret = new Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_8a6f238a6ece86ea: function() {
            const ret = new Error();
            return ret;
        },
        __wbg_new_b5d9e2fb389fef91: function(arg0, arg1) {
            try {
                var state0 = {a: arg0, b: arg1};
                var cb0 = (arg0, arg1) => {
                    const a = state0.a;
                    state0.a = 0;
                    try {
                        return wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___wasm_bindgen_1f3b1eaef9b9ff9e___JsValue__wasm_bindgen_1f3b1eaef9b9ff9e___JsValue_____(a, state0.b, arg0, arg1);
                    } finally {
                        state0.a = a;
                    }
                };
                const ret = new Promise(cb0);
                return ret;
            } finally {
                state0.a = state0.b = 0;
            }
        },
        __wbg_new_dca287b076112a51: function() {
            const ret = new Map();
            return ret;
        },
        __wbg_new_dd2b680c8bf6ae29: function(arg0) {
            const ret = new Uint8Array(arg0);
            return ret;
        },
        __wbg_new_from_slice_a3d2629dc1826784: function(arg0, arg1) {
            const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_no_args_1c7c842f08d00ebb: function(arg0, arg1) {
            const ret = new Function(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_with_byte_offset_and_length_aa261d9c9da49eb1: function(arg0, arg1, arg2) {
            const ret = new Uint8Array(arg0, arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_new_with_configuration_114cc8dc0d3b6519: function() { return handleError(function (arg0) {
            const ret = new RTCPeerConnection(arg0);
            return ret;
        }, arguments); },
        __wbg_new_with_into_underlying_source_b47f6a6a596a7f24: function(arg0, arg1) {
            const ret = new ReadableStream(IntoUnderlyingSource.__wrap(arg0), arg1);
            return ret;
        },
        __wbg_new_with_length_a2c39cbe88fd8ff1: function(arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return ret;
        },
        __wbg_new_with_options_4d98b7fe6f1234ea: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = new WebTransport(getStringFromWasm0(arg0, arg1), arg2);
            return ret;
        }, arguments); },
        __wbg_node_e1f24f89a7336c2e: function(arg0) {
            const ret = arg0.node;
            return ret;
        },
        __wbg_now_2c95c9de01293173: function(arg0) {
            const ret = arg0.now();
            return ret;
        },
        __wbg_now_a3af9a2f4bbaa4d1: function() {
            const ret = Date.now();
            return ret;
        },
        __wbg_objectStoreNames_d2c5d2377420ad78: function(arg0) {
            const ret = arg0.objectStoreNames;
            return ret;
        },
        __wbg_objectStore_d56e603390dcc165: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.objectStore(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_oldVersion_97e5f91fffb21425: function(arg0) {
            const ret = arg0.oldVersion;
            return ret;
        },
        __wbg_open_82db86fd5b087109: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.open(getStringFromWasm0(arg1, arg2), arg3 >>> 0);
            return ret;
        }, arguments); },
        __wbg_parse_708461a1feddfb38: function() { return handleError(function (arg0, arg1) {
            const ret = JSON.parse(getStringFromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg_performance_7a3ffd0b17f663ad: function(arg0) {
            const ret = arg0.performance;
            return ret;
        },
        __wbg_process_3975fd6c72f520aa: function(arg0) {
            const ret = arg0.process;
            return ret;
        },
        __wbg_prototypesetcall_bdcdcc5842e4d77d: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_push_8ffdcb2063340ba5: function(arg0, arg1) {
            const ret = arg0.push(arg1);
            return ret;
        },
        __wbg_put_b34701a38436f20a: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.put(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_queueMicrotask_0aa0a927f78f5d98: function(arg0) {
            const ret = arg0.queueMicrotask;
            return ret;
        },
        __wbg_queueMicrotask_5bb536982f78a56f: function(arg0) {
            queueMicrotask(arg0);
        },
        __wbg_randomFillSync_f8c153b79f285817: function() { return handleError(function (arg0, arg1) {
            arg0.randomFillSync(arg1);
        }, arguments); },
        __wbg_read_68fd377df67e19b0: function(arg0) {
            const ret = arg0.read();
            return ret;
        },
        __wbg_readable_f48737ebbe0d83d7: function(arg0) {
            const ret = arg0.readable;
            return ret;
        },
        __wbg_readyState_c000912ef3045df7: function(arg0) {
            const ret = arg0.readyState;
            return (__wbindgen_enum_RtcDataChannelState.indexOf(ret) + 1 || 5) - 1;
        },
        __wbg_ready_4a2ef790cf8ee5f8: function(arg0) {
            const ret = arg0.ready;
            return ret;
        },
        __wbg_ready_8484a7b5b5439603: function(arg0) {
            const ret = arg0.ready;
            return ret;
        },
        __wbg_removeEntry_d1cc9710704217eb: function(arg0, arg1, arg2) {
            const ret = arg0.removeEntry(getStringFromWasm0(arg1, arg2));
            return ret;
        },
        __wbg_removeEventListener_e63328781a5b9af9: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            arg0.removeEventListener(getStringFromWasm0(arg1, arg2), arg3);
        }, arguments); },
        __wbg_require_b74f47fc2d022fd6: function() { return handleError(function () {
            const ret = module.require;
            return ret;
        }, arguments); },
        __wbg_resolve_002c4b7d9d8f6b64: function(arg0) {
            const ret = Promise.resolve(arg0);
            return ret;
        },
        __wbg_respond_bf6ab10399ca8722: function() { return handleError(function (arg0, arg1) {
            arg0.respond(arg1 >>> 0);
        }, arguments); },
        __wbg_result_233b2d68aae87a05: function() { return handleError(function (arg0) {
            const ret = arg0.result;
            return ret;
        }, arguments); },
        __wbg_sdp_d49b2809185ccae2: function(arg0, arg1) {
            const ret = arg1.sdp;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_send_ec7fccacb8d4ed00: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.send(getArrayU8FromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_setItem_cf340bb2edbd3089: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.setItem(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_setLocalDescription_286acbf723f59b5c: function(arg0, arg1) {
            const ret = arg0.setLocalDescription(arg1);
            return ret;
        },
        __wbg_setRemoteDescription_225bc4358168e1f0: function(arg0, arg1) {
            const ret = arg0.setRemoteDescription(arg1);
            return ret;
        },
        __wbg_setTimeout_2b111259203a2623: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.setTimeout(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_setTimeout_eefe7f4c234b0c6b: function() { return handleError(function (arg0, arg1) {
            const ret = setTimeout(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_set_1eb0999cf5d27fc8: function(arg0, arg1, arg2) {
            const ret = arg0.set(arg1, arg2);
            return ret;
        },
        __wbg_set_3f1d0b984ed272ed: function(arg0, arg1, arg2) {
            arg0[arg1] = arg2;
        },
        __wbg_set_6cb8631f80447a67: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = Reflect.set(arg0, arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_set_binaryType_f4f87648fdda0dac: function(arg0, arg1) {
            arg0.binaryType = __wbindgen_enum_RtcDataChannelType[arg1];
        },
        __wbg_set_bufferedAmountLowThreshold_649a569c0fb51336: function(arg0, arg1) {
            arg0.bufferedAmountLowThreshold = arg1 >>> 0;
        },
        __wbg_set_cc56eefd2dd91957: function(arg0, arg1, arg2) {
            arg0.set(getArrayU8FromWasm0(arg1, arg2));
        },
        __wbg_set_certificates_c764f54e044e665e: function(arg0, arg1) {
            arg0.certificates = arg1;
        },
        __wbg_set_create_1f902c5936adde7d: function(arg0, arg1) {
            arg0.create = arg1 !== 0;
        },
        __wbg_set_create_c95ddca018fac9ce: function(arg0, arg1) {
            arg0.create = arg1 !== 0;
        },
        __wbg_set_data_d1381239efcb587a: function(arg0, arg1) {
            arg0.data = arg1;
        },
        __wbg_set_f43e577aea94465b: function(arg0, arg1, arg2) {
            arg0[arg1 >>> 0] = arg2;
        },
        __wbg_set_high_water_mark_a7ede9ba8be01a98: function(arg0, arg1) {
            arg0.highWaterMark = arg1;
        },
        __wbg_set_ice_servers_2fbbe72dcc5bb69a: function(arg0, arg1) {
            arg0.iceServers = arg1;
        },
        __wbg_set_id_541c66ff3ff08d76: function(arg0, arg1) {
            arg0.id = arg1;
        },
        __wbg_set_keep_existing_data_ac7fffea75f37b19: function(arg0, arg1) {
            arg0.keepExistingData = arg1 !== 0;
        },
        __wbg_set_negotiated_8a48a71eb810cad0: function(arg0, arg1) {
            arg0.negotiated = arg1 !== 0;
        },
        __wbg_set_onabort_5b85743a64489257: function(arg0, arg1) {
            arg0.onabort = arg1;
        },
        __wbg_set_onbufferedamountlow_2ae87a1aa500a50a: function(arg0, arg1) {
            arg0.onbufferedamountlow = arg1;
        },
        __wbg_set_onclose_cd1e79ee9a126bf3: function(arg0, arg1) {
            arg0.onclose = arg1;
        },
        __wbg_set_oncomplete_76d4a772a6c8cab6: function(arg0, arg1) {
            arg0.oncomplete = arg1;
        },
        __wbg_set_onconnectionstatechange_662fb34d742b54af: function(arg0, arg1) {
            arg0.onconnectionstatechange = arg1;
        },
        __wbg_set_ondatachannel_1c46b51a91f1578b: function(arg0, arg1) {
            arg0.ondatachannel = arg1;
        },
        __wbg_set_onerror_01fc830cd8567895: function(arg0, arg1) {
            arg0.onerror = arg1;
        },
        __wbg_set_onerror_d0db7c6491b9399d: function(arg0, arg1) {
            arg0.onerror = arg1;
        },
        __wbg_set_onerror_dc0e606b09e1792f: function(arg0, arg1) {
            arg0.onerror = arg1;
        },
        __wbg_set_onicecandidate_d7f0eeb668892055: function(arg0, arg1) {
            arg0.onicecandidate = arg1;
        },
        __wbg_set_onmessage_b37c5e7b9ca15286: function(arg0, arg1) {
            arg0.onmessage = arg1;
        },
        __wbg_set_onopen_5d8b1bc500a88ba1: function(arg0, arg1) {
            arg0.onopen = arg1;
        },
        __wbg_set_onsuccess_0edec1acb4124784: function(arg0, arg1) {
            arg0.onsuccess = arg1;
        },
        __wbg_set_onupgradeneeded_c887b74722b6ce77: function(arg0, arg1) {
            arg0.onupgradeneeded = arg1;
        },
        __wbg_set_onversionchange_34b86d0aaffbe107: function(arg0, arg1) {
            arg0.onversionchange = arg1;
        },
        __wbg_set_position_5836fe685f23de9d: function(arg0, arg1, arg2) {
            arg0.position = arg1 === 0 ? undefined : arg2;
        },
        __wbg_set_sdp_50fb460598980761: function(arg0, arg1, arg2) {
            arg0.sdp = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_type_76aecafd1e278305: function(arg0, arg1) {
            arg0.type = __wbindgen_enum_RtcSdpType[arg1];
        },
        __wbg_set_type_d1dca6d3dab1967f: function(arg0, arg1) {
            arg0.type = __wbindgen_enum_WriteCommandType[arg1];
        },
        __wbg_size_e05d31cc6049815f: function(arg0) {
            const ret = arg0.size;
            return ret;
        },
        __wbg_slice_8bbd46adb2100583: function(arg0, arg1, arg2) {
            const ret = arg0.slice(arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_slice_a4d15492574b99a1: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.slice(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_stack_0ed75d68575b0f3c: function(arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_static_accessor_GLOBAL_12837167ad935116: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_THIS_e628e89ab3b1c95f: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_a621d3dfbb60d0ce: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_f8727f0cf888e0bd: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_storage_6ef5028f2a840762: function(arg0) {
            const ret = arg0.storage;
            return ret;
        },
        __wbg_storage_c002b53bc4883299: function(arg0) {
            const ret = arg0.storage;
            return ret;
        },
        __wbg_stringify_8d1cc6ff383e8bae: function() { return handleError(function (arg0) {
            const ret = JSON.stringify(arg0);
            return ret;
        }, arguments); },
        __wbg_subarray_a96e1fef17ed23cb: function(arg0, arg1, arg2) {
            const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_text_6c01d6a72e29d7a7: function(arg0) {
            const ret = arg0.text();
            return ret;
        },
        __wbg_then_0d9fe2c7b1857d32: function(arg0, arg1, arg2) {
            const ret = arg0.then(arg1, arg2);
            return ret;
        },
        __wbg_then_b9e7b3b5f1a9e1b5: function(arg0, arg1) {
            const ret = arg0.then(arg1);
            return ret;
        },
        __wbg_toJSON_6d32b3fcb45814f2: function(arg0) {
            const ret = arg0.toJSON();
            return ret;
        },
        __wbg_toString_029ac24421fd7a24: function(arg0) {
            const ret = arg0.toString();
            return ret;
        },
        __wbg_transaction_55ceb96f4b852417: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.transaction(getStringFromWasm0(arg1, arg2), __wbindgen_enum_IdbTransactionMode[arg3]);
            return ret;
        }, arguments); },
        __wbg_userAgent_34463fd660ba4a2a: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.userAgent;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_userAgent_bfce7c11898c1a76: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.userAgent;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_versions_4e31226f5e8dc909: function(arg0) {
            const ret = arg0.versions;
            return ret;
        },
        __wbg_view_6c32e7184b8606ad: function(arg0) {
            const ret = arg0.view;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_warn_479b8bbb8337357b: function(arg0, arg1) {
            var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
            wasm.__wbindgen_free(arg0, arg1 * 4, 4);
            console.warn(...v0);
        },
        __wbg_webnode_new: function(arg0) {
            const ret = WebNode.__wrap(arg0);
            return ret;
        },
        __wbg_writable_48ed470a7316152a: function(arg0) {
            const ret = arg0.writable;
            return ret;
        },
        __wbg_write_3b10b2d633031cad: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.write(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_write_4463a833fb89f0b8: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.write(arg1);
            return ret;
        }, arguments); },
        __wbg_write_4dbba5e5426abaf4: function(arg0, arg1) {
            const ret = arg0.write(arg1);
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 2393, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 2394, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_1f3b1eaef9b9ff9e___closure__destroy___dyn_core_7d5f0a2ba6a62c33___ops__function__FnMut__web_sys_93005bece23d88e1___features__gen_MessageEvent__MessageEvent____Output_______, wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___web_sys_93005bece23d88e1___features__gen_MessageEvent__MessageEvent_____);
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 2393, function: Function { arguments: [NamedExternref("RTCDataChannelEvent")], shim_idx: 2394, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_1f3b1eaef9b9ff9e___closure__destroy___dyn_core_7d5f0a2ba6a62c33___ops__function__FnMut__web_sys_93005bece23d88e1___features__gen_MessageEvent__MessageEvent____Output_______, wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___web_sys_93005bece23d88e1___features__gen_MessageEvent__MessageEvent_____);
            return ret;
        },
        __wbindgen_cast_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 2393, function: Function { arguments: [NamedExternref("RTCPeerConnectionIceEvent")], shim_idx: 2394, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_1f3b1eaef9b9ff9e___closure__destroy___dyn_core_7d5f0a2ba6a62c33___ops__function__FnMut__web_sys_93005bece23d88e1___features__gen_MessageEvent__MessageEvent____Output_______, wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___web_sys_93005bece23d88e1___features__gen_MessageEvent__MessageEvent_____);
            return ret;
        },
        __wbindgen_cast_0000000000000004: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 2537, function: Function { arguments: [], shim_idx: 2538, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_1f3b1eaef9b9ff9e___closure__destroy___dyn_core_7d5f0a2ba6a62c33___ops__function__FnMut_____Output_______, wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke_______1_);
            return ret;
        },
        __wbindgen_cast_0000000000000005: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 3127, function: Function { arguments: [Externref], shim_idx: 3128, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_1f3b1eaef9b9ff9e___closure__destroy___dyn_core_7d5f0a2ba6a62c33___ops__function__FnMut__wasm_bindgen_1f3b1eaef9b9ff9e___JsValue____Output_______, wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___wasm_bindgen_1f3b1eaef9b9ff9e___JsValue_____);
            return ret;
        },
        __wbindgen_cast_0000000000000006: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 3181, function: Function { arguments: [], shim_idx: 3182, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_1f3b1eaef9b9ff9e___closure__destroy___dyn_core_7d5f0a2ba6a62c33___ops__function__FnMut_____Output________1_, wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke_______2_);
            return ret;
        },
        __wbindgen_cast_0000000000000007: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 919, function: Function { arguments: [NamedExternref("Event")], shim_idx: 920, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_1f3b1eaef9b9ff9e___closure__destroy___dyn_core_7d5f0a2ba6a62c33___ops__function__FnMut__web_sys_93005bece23d88e1___features__gen_CloseEvent__CloseEvent____Output_______, wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___web_sys_93005bece23d88e1___features__gen_CloseEvent__CloseEvent_____);
            return ret;
        },
        __wbindgen_cast_0000000000000008: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 919, function: Function { arguments: [], shim_idx: 922, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_1f3b1eaef9b9ff9e___closure__destroy___dyn_core_7d5f0a2ba6a62c33___ops__function__FnMut__web_sys_93005bece23d88e1___features__gen_CloseEvent__CloseEvent____Output_______, wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke______);
            return ret;
        },
        __wbindgen_cast_0000000000000009: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_000000000000000a: function(arg0) {
            // Cast intrinsic for `I64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_000000000000000b: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
            const ret = getArrayU8FromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_000000000000000c: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_000000000000000d: function(arg0) {
            // Cast intrinsic for `U64 -> Externref`.
            const ret = BigInt.asUintN(64, arg0);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./swarmdrop_web_bg.js": import0,
    };
}

function wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke_______1_(arg0, arg1) {
    wasm.wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke_______1_(arg0, arg1);
}

function wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke_______2_(arg0, arg1) {
    wasm.wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke_______2_(arg0, arg1);
}

function wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke______(arg0, arg1) {
    wasm.wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke______(arg0, arg1);
}

function wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___web_sys_93005bece23d88e1___features__gen_MessageEvent__MessageEvent_____(arg0, arg1, arg2) {
    wasm.wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___web_sys_93005bece23d88e1___features__gen_MessageEvent__MessageEvent_____(arg0, arg1, arg2);
}

function wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___wasm_bindgen_1f3b1eaef9b9ff9e___JsValue_____(arg0, arg1, arg2) {
    wasm.wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___wasm_bindgen_1f3b1eaef9b9ff9e___JsValue_____(arg0, arg1, arg2);
}

function wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___web_sys_93005bece23d88e1___features__gen_CloseEvent__CloseEvent_____(arg0, arg1, arg2) {
    wasm.wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___web_sys_93005bece23d88e1___features__gen_CloseEvent__CloseEvent_____(arg0, arg1, arg2);
}

function wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___wasm_bindgen_1f3b1eaef9b9ff9e___JsValue__wasm_bindgen_1f3b1eaef9b9ff9e___JsValue_____(arg0, arg1, arg2, arg3) {
    wasm.wasm_bindgen_1f3b1eaef9b9ff9e___convert__closures_____invoke___wasm_bindgen_1f3b1eaef9b9ff9e___JsValue__wasm_bindgen_1f3b1eaef9b9ff9e___JsValue_____(arg0, arg1, arg2, arg3);
}


const __wbindgen_enum_IdbTransactionMode = ["readonly", "readwrite", "versionchange", "readwriteflush", "cleanup"];


const __wbindgen_enum_ReadableStreamType = ["bytes"];


const __wbindgen_enum_RtcDataChannelState = ["connecting", "open", "closing", "closed"];


const __wbindgen_enum_RtcDataChannelType = ["arraybuffer", "blob"];


const __wbindgen_enum_RtcPeerConnectionState = ["closed", "failed", "disconnected", "new", "connecting", "connected"];


const __wbindgen_enum_RtcSdpType = ["offer", "pranswer", "answer", "rollback"];


const __wbindgen_enum_WriteCommandType = ["write", "seek", "truncate"];
const IntoUnderlyingByteSourceFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_intounderlyingbytesource_free(ptr >>> 0, 1));
const IntoUnderlyingSinkFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_intounderlyingsink_free(ptr >>> 0, 1));
const IntoUnderlyingSourceFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_intounderlyingsource_free(ptr >>> 0, 1));
const WebNodeFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_webnode_free(ptr >>> 0, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(state => state.dtor(state.a, state.b));

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches && builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function makeMutClosure(arg0, arg1, dtor, f) {
    const state = { a: arg0, b: arg1, cnt: 1, dtor };
    const real = (...args) => {

        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        const a = state.a;
        state.a = 0;
        try {
            return f(a, state.b, ...args);
        } finally {
            state.a = a;
            real._wbg_cb_unref();
        }
    };
    real._wbg_cb_unref = () => {
        if (--state.cnt === 0) {
            state.dtor(state.a, state.b);
            state.a = 0;
            CLOSURE_DTORS.unregister(state);
        }
    };
    CLOSURE_DTORS.register(real, state, state);
    return real;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    for (let i = 0; i < array.length; i++) {
        const add = addToExternrefTable0(array[i]);
        getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasm;
function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('swarmdrop_web_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
