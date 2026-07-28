//! Rust → JS 的统一序列化器。
//!
//! **所有跨 wasm 边界的 serde 序列化都必须走这里**，不要直接调
//! [`serde_wasm_bindgen::to_value`]。

use serde::Serialize;
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
}
