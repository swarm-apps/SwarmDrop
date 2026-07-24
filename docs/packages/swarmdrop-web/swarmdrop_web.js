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
     * 双方写配对记录。身份 pin 由握手强制（连到的必然是 `inviter_id`）。成功返回已配对对端的
     * NodeId（base58）；确认发生在**邀请方**侧，浏览器侧无需交互。配对后该对端进入本机信任
     * 表，双向传输（收 / 发）不再被 `NotPaired` 拦。
     * @param {string} invite
     * @returns {Promise<string>}
     */
    connect_invite(invite) {
        const ptr0 = passStringToWasm0(invite, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_connect_invite(this.__wbg_ptr, ptr0, len0);
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
     * 拨得到本机，本机需先经 [`relays_ensure`](Self::relays_ensure) 在某 helper 上建 reservation
     * （等到 `active`），否则邀请里无可拨地址、消费方连不上。
     * @param {boolean} local_only
     * @returns {string}
     */
    generate_invite(local_only) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.webnode_generate_invite(this.__wbg_ptr, local_only);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
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
     * relay 状态变化流：每次变化产出一份全量快照（可直接 setState）。
     * 可多次调用（每次独立订阅），与 `events()` 的单点消费不同。
     * @returns {ReadableStream<RelayInfoJson[]>}
     */
    relays_changed() {
        const ret = wasm.webnode_relays_changed(this.__wbg_ptr);
        return ret;
    }
    /**
     * 撤销 relay 意图（[`relays_ensure`](Self::relays_ensure) 的对称面）。
     *
     * **真撤销**而非停止等待：停止后台收敛重试、关闭 circuit listener、立刻
     * 断开与该 helper 的连接（含中止在途拨号），条目从状态集合消失。
     * @param {string} helper_id
     * @returns {Promise<void>}
     */
    relays_drop(helper_id) {
        const ptr0 = passStringToWasm0(helper_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_relays_drop(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * 登记一个 relay helper 的常驻可达意图（幂等，同步返回）。
     *
     * 浏览器被动接收连接的唯一入口。拨号 / reservation / 断线重建由 core 的
     * InfraSupervisor 统一收敛（最迟 1s 内启动第一轮，失败退避重试）；进度经
     * [`relays_state`](Self::relays_state) / [`relays_changed`](Self::relays_changed)
     * 观测，或用 [`relays_until_active`](Self::relays_until_active) 等首次建立。
     *
     * 返回 helper 的 base58 NodeId——即 `relays_drop` / `relays_until_active` 的
     * 入参，调用方直接串联，无需自行解析 multiaddr 的 `/p2p/` 段。
     * @param {string} helper_addr
     * @returns {string}
     */
    relays_ensure(helper_addr) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(helper_addr, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.webnode_relays_ensure(this.__wbg_ptr, ptr0, len0);
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
     * 全量 relay 状态快照（`{ id, state, circuitAddr?, lastError? }[]`）。
     * @returns {RelayInfoJson[]}
     */
    relays_state() {
        const ret = wasm.webnode_relays_state(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * 等待某 relay 首次进入 `active`，resolve 出 circuit 可达地址（内核拼装）。
     *
     * 观察到 `failed` 时**立即 reject**（把「要不要再等下一轮退避」还给调用方），
     * 意图保留——要停止后台收敛请调 [`relays_drop`](Self::relays_drop)。
     * `signal`（可选）：abort 只是不再等待，同样不改变意图生命周期。
     * 不传 signal 时 30s 兜底超时保证 Promise 有限时间内 settle。
     * @param {string} helper_id
     * @param {AbortSignal | null} [signal]
     * @returns {Promise<string>}
     */
    relays_until_active(helper_id, signal) {
        const ptr0 = passStringToWasm0(helper_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_relays_until_active(this.__wbg_ptr, ptr0, len0, isLikeNone(signal) ? 0 : addToExternrefTable0(signal));
        return ret;
    }
    /**
     * 响应一个入站配对请求（`accept=true` 接受并写配对记录、CAS 消费 invite / `false` 拒绝）。
     * @param {string} pending_id
     * @param {boolean} accept
     * @returns {Promise<void>}
     */
    respond_pairing_request(pending_id, accept) {
        const ptr0 = passStringToWasm0(pending_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webnode_respond_pairing_request(this.__wbg_ptr, ptr0, len0, accept);
        return ret;
    }
    /**
     * 手动发起断点续传（对某 suspended 会话）。
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
     * 建节点：持久化身份（Window=localStorage / Worker=OPFS）→ 包 core 组合根 [`start_node`]
     * （Browser [`EndpointProfile`] + Web 端口）→ 完整 [`NetManager`] + 3 协议 Router（含
     * pairing）。**须在主线程 Window 跑**——webrtc-websys dial 碰 window，Worker 里会 panic。
     * @returns {Promise<WebNode>}
     */
    static spawn() {
        const ret = wasm.webnode_spawn();
        return ret;
    }
}
if (Symbol.dispose) WebNode.prototype[Symbol.dispose] = WebNode.prototype.free;

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
        __wbg_String_8f0eb39a4a4c2f66: function(arg0, arg1) {
            const ret = String(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_WorkerGlobalScope_f6889a7bbe9f0d2d: function(arg0) {
            const ret = arg0.WorkerGlobalScope;
            return ret;
        },
        __wbg___wbindgen_debug_string_0bc8482c6e3508ae: function(arg0, arg1) {
            const ret = debugString(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_is_function_0095a73b8b156f76: function(arg0) {
            const ret = typeof(arg0) === 'function';
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
        __wbg_aborted_0b67c37a14dbbc89: function(arg0) {
            const ret = arg0.aborted;
            return ret;
        },
        __wbg_addEventListener_3acb0aad4483804c: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3);
        }, arguments); },
        __wbg_arrayBuffer_05ce1af23e9064e8: function(arg0) {
            const ret = arg0.arrayBuffer();
            return ret;
        },
        __wbg_buffer_26d0910f3a5bc899: function(arg0) {
            const ret = arg0.buffer;
            return ret;
        },
        __wbg_bufferedAmount_0d42d0dc52062133: function(arg0) {
            const ret = arg0.bufferedAmount;
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
        __wbg_channel_82b58a29dba55e8a: function(arg0) {
            const ret = arg0.channel;
            return ret;
        },
        __wbg_clearInterval_905868a11bace7cc: function(arg0, arg1) {
            arg0.clearInterval(arg1);
        },
        __wbg_clearInterval_c75df0651e74fbb8: function(arg0, arg1) {
            arg0.clearInterval(arg1);
        },
        __wbg_clearTimeout_5e42188b495715bb: function() { return handleError(function (arg0, arg1) {
            arg0.clearTimeout(arg1);
        }, arguments); },
        __wbg_clearTimeout_96804de0ab838f26: function(arg0) {
            const ret = clearTimeout(arg0);
            return ret;
        },
        __wbg_close_06dfa0a815b9d71f: function() { return handleError(function (arg0) {
            arg0.close();
        }, arguments); },
        __wbg_close_83fb809aca3de7f9: function(arg0) {
            const ret = arg0.close();
            return ret;
        },
        __wbg_close_a79afee31de55b36: function() { return handleError(function (arg0) {
            arg0.close();
        }, arguments); },
        __wbg_close_eef7356f70a62f3c: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            arg0.close(arg1, getStringFromWasm0(arg2, arg3));
        }, arguments); },
        __wbg_close_f9ba12c30bbb456f: function(arg0) {
            arg0.close();
        },
        __wbg_createDataChannel_1175bbde394c8293: function(arg0, arg1, arg2, arg3) {
            const ret = arg0.createDataChannel(getStringFromWasm0(arg1, arg2), arg3);
            return ret;
        },
        __wbg_createDataChannel_5b6887f64b34cde3: function(arg0, arg1, arg2) {
            const ret = arg0.createDataChannel(getStringFromWasm0(arg1, arg2));
            return ret;
        },
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
        __wbg_document_ee35a3d3ae34ef6c: function(arg0) {
            const ret = arg0.document;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_enqueue_2c63f2044f257c3e: function() { return handleError(function (arg0, arg1) {
            arg0.enqueue(arg1);
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
        __wbg_generateCertificate_451abc23dcbd6480: function() { return handleError(function (arg0) {
            const ret = RTCPeerConnection.generateCertificate(arg0);
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
        __wbg_getRandomValues_a8ddca022803a145: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_getRandomValues_b3f15fcbfabb0f8b: function() { return handleError(function (arg0, arg1) {
            arg0.getRandomValues(arg1);
        }, arguments); },
        __wbg_getRandomValues_c1a6b3fa4f05f846: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_getTime_1e3cd1391c5c3995: function(arg0) {
            const ret = arg0.getTime();
            return ret;
        },
        __wbg_get_b3ed3ad4be2bc8ac: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_hostname_0c450e33386895ba: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.hostname;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_id_5a5e3288567f6f1f: function(arg0) {
            const ret = arg0.id;
            return isLikeNone(ret) ? 0xFFFFFF : ret;
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
        __wbg_isSecureContext_196d8462fd333d8d: function(arg0) {
            const ret = arg0.isSecureContext;
            return ret;
        },
        __wbg_isSecureContext_1e186b850f07cfb3: function(arg0) {
            const ret = arg0.isSecureContext;
            return ret;
        },
        __wbg_lastModified_a5cfce993c651681: function(arg0) {
            const ret = arg0.lastModified;
            return ret;
        },
        __wbg_length_32ed9a279acd054c: function(arg0) {
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
        __wbg_location_f4dc29bc7f202edf: function(arg0) {
            const ret = arg0.location;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_log_e51ef223c244b133: function(arg0, arg1) {
            var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
            wasm.__wbindgen_free(arg0, arg1 * 4, 4);
            console.log(...v0);
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
        __wbg_navigator_43be698ba96fc088: function(arg0) {
            const ret = arg0.navigator;
            return ret;
        },
        __wbg_navigator_4478931f32ebca57: function(arg0) {
            const ret = arg0.navigator;
            return ret;
        },
        __wbg_new_057993d5b5e07835: function() { return handleError(function (arg0, arg1) {
            const ret = new WebSocket(getStringFromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg_new_0_73afc35eb544e539: function() {
            const ret = new Date();
            return ret;
        },
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
                        return wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___wasm_bindgen_93f1b4d14c46ee0b___JsValue__wasm_bindgen_93f1b4d14c46ee0b___JsValue_____(a, state0.b, arg0, arg1);
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
        __wbg_readyState_1bb73ec7b8a54656: function(arg0) {
            const ret = arg0.readyState;
            return ret;
        },
        __wbg_readyState_c000912ef3045df7: function(arg0) {
            const ret = arg0.readyState;
            return (__wbindgen_enum_RtcDataChannelState.indexOf(ret) + 1 || 5) - 1;
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
        __wbg_sdp_d49b2809185ccae2: function(arg0, arg1) {
            const ret = arg1.sdp;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_send_542f95dea2df7994: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.send(getArrayU8FromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_send_ec7fccacb8d4ed00: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.send(getArrayU8FromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_setInterval_0e3c8fcec0876733: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.setInterval(arg1, arg2, ...arg3);
            return ret;
        }, arguments); },
        __wbg_setInterval_b471c2130618eef6: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.setInterval(arg1, arg2, ...arg3);
            return ret;
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
        __wbg_set_3f1d0b984ed272ed: function(arg0, arg1, arg2) {
            arg0[arg1] = arg2;
        },
        __wbg_set_6cb8631f80447a67: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = Reflect.set(arg0, arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_set_binaryType_5bbf62e9f705dc1a: function(arg0, arg1) {
            arg0.binaryType = __wbindgen_enum_BinaryType[arg1];
        },
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
        __wbg_set_id_541c66ff3ff08d76: function(arg0, arg1) {
            arg0.id = arg1;
        },
        __wbg_set_keep_existing_data_ac7fffea75f37b19: function(arg0, arg1) {
            arg0.keepExistingData = arg1 !== 0;
        },
        __wbg_set_negotiated_8a48a71eb810cad0: function(arg0, arg1) {
            arg0.negotiated = arg1 !== 0;
        },
        __wbg_set_onbufferedamountlow_2ae87a1aa500a50a: function(arg0, arg1) {
            arg0.onbufferedamountlow = arg1;
        },
        __wbg_set_onclose_cd1e79ee9a126bf3: function(arg0, arg1) {
            arg0.onclose = arg1;
        },
        __wbg_set_onclose_d382f3e2c2b850eb: function(arg0, arg1) {
            arg0.onclose = arg1;
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
        __wbg_set_onerror_377f18bf4569bf85: function(arg0, arg1) {
            arg0.onerror = arg1;
        },
        __wbg_set_onmessage_2114aa5f4f53051e: function(arg0, arg1) {
            arg0.onmessage = arg1;
        },
        __wbg_set_onmessage_b37c5e7b9ca15286: function(arg0, arg1) {
            arg0.onmessage = arg1;
        },
        __wbg_set_onopen_5d8b1bc500a88ba1: function(arg0, arg1) {
            arg0.onopen = arg1;
        },
        __wbg_set_onopen_b7b52d519d6c0f11: function(arg0, arg1) {
            arg0.onopen = arg1;
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
        __wbg_subarray_a96e1fef17ed23cb: function(arg0, arg1, arg2) {
            const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_target_521be630ab05b11e: function(arg0) {
            const ret = arg0.target;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
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
        __wbg_toString_029ac24421fd7a24: function(arg0) {
            const ret = arg0.toString();
            return ret;
        },
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
        __wbg_write_3b10b2d633031cad: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.write(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_write_4463a833fb89f0b8: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.write(arg1);
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 1757, function: Function { arguments: [], shim_idx: 1758, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_93f1b4d14c46ee0b___closure__destroy___dyn_core_9b3796e30d99ddb7___ops__function__FnMut_____Output_______, wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke______);
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 1844, function: Function { arguments: [NamedExternref("CloseEvent")], shim_idx: 1845, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_93f1b4d14c46ee0b___closure__destroy___dyn_core_9b3796e30d99ddb7___ops__function__FnMut__web_sys_eb16ac75ad3859fe___features__gen_CloseEvent__CloseEvent____Output_______, wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___web_sys_eb16ac75ad3859fe___features__gen_CloseEvent__CloseEvent_____);
            return ret;
        },
        __wbindgen_cast_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 1844, function: Function { arguments: [NamedExternref("Event")], shim_idx: 1845, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_93f1b4d14c46ee0b___closure__destroy___dyn_core_9b3796e30d99ddb7___ops__function__FnMut__web_sys_eb16ac75ad3859fe___features__gen_CloseEvent__CloseEvent____Output_______, wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___web_sys_eb16ac75ad3859fe___features__gen_CloseEvent__CloseEvent_____);
            return ret;
        },
        __wbindgen_cast_0000000000000004: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 1844, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 1845, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_93f1b4d14c46ee0b___closure__destroy___dyn_core_9b3796e30d99ddb7___ops__function__FnMut__web_sys_eb16ac75ad3859fe___features__gen_CloseEvent__CloseEvent____Output_______, wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___web_sys_eb16ac75ad3859fe___features__gen_CloseEvent__CloseEvent_____);
            return ret;
        },
        __wbindgen_cast_0000000000000005: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 1937, function: Function { arguments: [NamedExternref("Event")], shim_idx: 1938, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_93f1b4d14c46ee0b___closure__destroy___dyn_core_9b3796e30d99ddb7___ops__function__FnMut__web_sys_eb16ac75ad3859fe___features__gen_MessageEvent__MessageEvent____Output_______, wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___web_sys_eb16ac75ad3859fe___features__gen_MessageEvent__MessageEvent_____);
            return ret;
        },
        __wbindgen_cast_0000000000000006: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 1937, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 1938, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_93f1b4d14c46ee0b___closure__destroy___dyn_core_9b3796e30d99ddb7___ops__function__FnMut__web_sys_eb16ac75ad3859fe___features__gen_MessageEvent__MessageEvent____Output_______, wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___web_sys_eb16ac75ad3859fe___features__gen_MessageEvent__MessageEvent_____);
            return ret;
        },
        __wbindgen_cast_0000000000000007: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 1937, function: Function { arguments: [NamedExternref("RTCDataChannelEvent")], shim_idx: 1938, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_93f1b4d14c46ee0b___closure__destroy___dyn_core_9b3796e30d99ddb7___ops__function__FnMut__web_sys_eb16ac75ad3859fe___features__gen_MessageEvent__MessageEvent____Output_______, wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___web_sys_eb16ac75ad3859fe___features__gen_MessageEvent__MessageEvent_____);
            return ret;
        },
        __wbindgen_cast_0000000000000008: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 2540, function: Function { arguments: [Externref], shim_idx: 2541, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_93f1b4d14c46ee0b___closure__destroy___dyn_core_9b3796e30d99ddb7___ops__function__FnMut__wasm_bindgen_93f1b4d14c46ee0b___JsValue____Output_______, wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___wasm_bindgen_93f1b4d14c46ee0b___JsValue_____);
            return ret;
        },
        __wbindgen_cast_0000000000000009: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { dtor_idx: 2594, function: Function { arguments: [], shim_idx: 2595, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen_93f1b4d14c46ee0b___closure__destroy___dyn_core_9b3796e30d99ddb7___ops__function__FnMut_____Output________1_, wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke_______1_);
            return ret;
        },
        __wbindgen_cast_000000000000000a: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_000000000000000b: function(arg0) {
            // Cast intrinsic for `I64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_000000000000000c: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
            const ret = getArrayU8FromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_000000000000000d: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_000000000000000e: function(arg0) {
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

function wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke______(arg0, arg1) {
    wasm.wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke______(arg0, arg1);
}

function wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke_______1_(arg0, arg1) {
    wasm.wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke_______1_(arg0, arg1);
}

function wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___web_sys_eb16ac75ad3859fe___features__gen_CloseEvent__CloseEvent_____(arg0, arg1, arg2) {
    wasm.wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___web_sys_eb16ac75ad3859fe___features__gen_CloseEvent__CloseEvent_____(arg0, arg1, arg2);
}

function wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___web_sys_eb16ac75ad3859fe___features__gen_MessageEvent__MessageEvent_____(arg0, arg1, arg2) {
    wasm.wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___web_sys_eb16ac75ad3859fe___features__gen_MessageEvent__MessageEvent_____(arg0, arg1, arg2);
}

function wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___wasm_bindgen_93f1b4d14c46ee0b___JsValue_____(arg0, arg1, arg2) {
    wasm.wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___wasm_bindgen_93f1b4d14c46ee0b___JsValue_____(arg0, arg1, arg2);
}

function wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___wasm_bindgen_93f1b4d14c46ee0b___JsValue__wasm_bindgen_93f1b4d14c46ee0b___JsValue_____(arg0, arg1, arg2, arg3) {
    wasm.wasm_bindgen_93f1b4d14c46ee0b___convert__closures_____invoke___wasm_bindgen_93f1b4d14c46ee0b___JsValue__wasm_bindgen_93f1b4d14c46ee0b___JsValue_____(arg0, arg1, arg2, arg3);
}


const __wbindgen_enum_BinaryType = ["blob", "arraybuffer"];


const __wbindgen_enum_ReadableStreamType = ["bytes"];


const __wbindgen_enum_RtcDataChannelState = ["connecting", "open", "closing", "closed"];


const __wbindgen_enum_RtcDataChannelType = ["arraybuffer", "blob"];


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
