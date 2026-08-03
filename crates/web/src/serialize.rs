//! 跨 wasm 边界的统一 serde 编解码器。
//!
//! **所有跨边界的 serde 转换都必须走这里**，不要直接调 [`serde_wasm_bindgen`] 的
//! `to_value` / `from_value`——出站那一侧有一个必须带的选项（见 [`to_js`]），
//! 而入站那一侧要的是统一的错误形态。

use serde::{Deserialize, Serialize};
#[cfg(test)]
use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;

/// 与生成的 `.d.ts` 契约一致的序列化器。
///
/// ⚠️ `serialize_maps_as_objects(true)` 这一行是本模块存在的全部理由。
///
/// `serde_wasm_bindgen` 默认把 serde 的 **map** 序列化成 JS `Map`。这对真正的
/// `HashMap` 尚可争论，但带 `#[serde(flatten)]` 的**结构体走的正是 map 路径**
/// ——serde 无法静态知道展开后的键，只能按 map 输出。本仓有两个这样的类型跨边界：
/// [`Device`](crate::types::Device) 与 `PairedDeviceInfo`（都 flatten 了 `OsInfo`）。
///
/// 于是 `paired_devices()` 返回的是一串 JS `Map`，而 `swarmdrop_web.d.ts` 声明的是
/// `Device = { peerId: string, … } & OsInfo`——普通对象。类型层看不出任何问题，
/// 运行时 `device.peerId` 却是 `undefined`：**字段一个都读不到**。
///
/// 2026-07-28 实证的表现（Web 端 #80）：已配对设备恒显示「离线」、传输条上永远是
/// 「连接类型未知」。`JSON.stringify(devices)` 得到 `[{}]` 是最快的判据——
/// `Map` 没有自有可枚举属性。
///
/// 判据很硬：`.d.ts` 里既没有 `Map<` 也没有 `Record<`，所以**任何**跨边界的值
/// 都该是普通对象。
pub(crate) fn to_js<T: Serialize + ?Sized>(
    value: &T,
) -> Result<JsValue, serde_wasm_bindgen::Error> {
    value.serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
}

/// JS → Rust。入站方向**没有**出站那个 `serialize_maps_as_objects` 的对应项：
/// 反序列化器本来就接受普通对象，普通对象正是 `.d.ts` 声明的形状。
///
/// 存在的理由只有一条：入口收在一处，于是「跨边界的 serde 转换都走 `crate::serialize`」
/// 这句话对两个方向都成立，不必让读代码的人去分辨哪个方向有讲究、哪个没有。
pub(crate) fn from_js<T: for<'de> Deserialize<'de>>(
    value: JsValue,
) -> Result<T, serde_wasm_bindgen::Error> {
    serde_wasm_bindgen::from_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    /// 带 `#[serde(flatten)]` 的结构体必须落成**普通对象**，不能是 JS `Map`。
    ///
    /// 这是 2026-07-28 那个「Web 端已配对设备恒显示离线 / 连接类型未知」的回归守卫：
    /// 默认序列化器把 flatten 走的 map 路径输出成 `Map`，于是 `.d.ts` 声明的每个字段
    /// 在运行时都读不到，而 TS 那侧完全看不出问题。
    ///
    /// 断言分三层，缺一不可：不是 `Map`（根因）、能按名取到普通字段（`status`）、
    /// 也能取到**被 flatten 展开**的字段（`platform`，来自 `OsInfo`）。
    /// 跑法：`wasm-pack test --headless --chrome -p swarmdrop-web`。
    #[wasm_bindgen_test]
    fn flattened_struct_serializes_as_plain_object() {
        #[derive(serde::Serialize)]
        struct Inner {
            platform: String,
        }
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Outer {
            peer_id: String,
            #[serde(flatten)]
            inner: Inner,
            status: String,
        }

        let value = to_js(&Outer {
            peer_id: "12D3KooTest".to_owned(),
            inner: Inner {
                platform: "web".to_owned(),
            },
            status: "online".to_owned(),
        })
        .expect("序列化不应失败");

        assert!(
            !value.is_instance_of::<js_sys::Map>(),
            "flatten 的结构体不能序列化成 JS Map——那会让所有字段在 JS 侧读不到"
        );

        let obj: &js_sys::Object = value.unchecked_ref();
        let get = |key: &str| js_sys::Reflect::get(obj, &JsValue::from_str(key)).ok();

        assert_eq!(
            get("peerId").and_then(|v| v.as_string()).as_deref(),
            Some("12D3KooTest"),
            "普通字段应可按名读取"
        );
        assert_eq!(
            get("status").and_then(|v| v.as_string()).as_deref(),
            Some("online")
        );
        assert_eq!(
            get("platform").and_then(|v| v.as_string()).as_deref(),
            Some("web"),
            "被 flatten 展开的字段同样要能按名读取"
        );
    }

    /// `DeviceReceivePolicy` 能从**普通 JS 对象**反序列化回来。
    ///
    /// 这是 [`from_js`] 的守卫，与上面那条序列化守卫成对：出站那条钉的是「别输出成 `Map`」，
    /// 这条钉的是「`.d.ts` 声明的那个形状真的收得回来」。
    ///
    /// 它守的是一条**没有编译期保护**的缝：`typescript_type = "DeviceReceivePolicy"` 里那个
    /// 类型名是个**字符串，wasm-bindgen 不校验**——TS 那侧按 `.d.ts` 传得再对，Rust 这侧的
    /// serde 属性（`rename_all` / `#[serde(default)]`）改了也不会有人报错，只会在运行时
    /// 变成一句「解析收件策略失败」。
    ///
    /// 断言分三层：必填字段收得到、`#[serde(default)]` 的字段缺席时不报错（前端只发它编辑过
    /// 的那几项时正是这样）、`Option<u64>` 收得到 JS 的 Number（`.d.ts` 里它被重映射成
    /// `number`，不是 BigInt）。
    ///
    /// 跑法见 `dev-notes/knowledge/toolchain.md` 的「跑 crates/web 的 wasm 测试」。
    #[wasm_bindgen_test]
    fn receive_policy_deserializes_from_plain_object() {
        use swarmdrop_host::device::DeviceReceivePolicy;

        let obj = js_sys::Object::new();
        let set = |k: &str, v: JsValue| {
            js_sys::Reflect::set(&obj, &JsValue::from_str(k), &v).expect("set 不应失败");
        };
        set("autoAccept", JsValue::TRUE);
        set("requireConfirmation", JsValue::FALSE);
        set("allowDirectories", JsValue::TRUE);
        set("allowRelayAutoAccept", JsValue::FALSE);
        set("allowMcpSendToDevice", JsValue::FALSE);
        set("maxTransferBytes", JsValue::from_f64(536_870_912.0));
        // saveBehavior / defaultSaveLocation / allowMcpAcceptFromDevice / expiresAt 刻意不给：
        // 它们都带 `#[serde(default)]`，缺席必须能过。

        let policy: DeviceReceivePolicy = from_js(obj.into()).expect("普通对象应能反序列化");

        assert!(policy.auto_accept);
        assert!(!policy.require_confirmation);
        assert!(policy.allow_directories);
        assert_eq!(policy.max_transfer_bytes, Some(536_870_912));
        assert_eq!(policy.expires_at, None, "缺席的可选字段应落到默认值");
    }

    /// `DeviceTrustLevel` 能从**裸字符串**反序列化回来（`.d.ts` 里它是字符串字面量联合）。
    #[wasm_bindgen_test]
    fn trust_level_deserializes_from_string() {
        use swarmdrop_host::device::DeviceTrustLevel;

        for (text, expected) in [
            ("owned", DeviceTrustLevel::Owned),
            ("collaborator", DeviceTrustLevel::Collaborator),
            ("temporary", DeviceTrustLevel::Temporary),
            ("blocked", DeviceTrustLevel::Blocked),
        ] {
            let level: DeviceTrustLevel =
                from_js(JsValue::from_str(text)).expect("字符串应能反序列化");
            assert_eq!(level, expected, "{text} 应映射到对应级别");
        }

        assert!(
            from_js::<DeviceTrustLevel>(JsValue::from_str("nonsense")).is_err(),
            "未知级别应报错而不是静默落到某个默认值"
        );
    }
}
