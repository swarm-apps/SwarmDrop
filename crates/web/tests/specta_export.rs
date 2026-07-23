//! TS 类型导出：把 [`WebTransferEvent`] / [`OfferJson`]（含整棵嵌套 transfer DTO 树）导成
//! `static/types/bindings.ts`（生成物入库，node.rs 经 `typescript_custom_section` 注入 .d.ts）。
//!
//! 跑法：`cargo test -p swarmdrop-web --features specta --test specta_export`（native）。
//! bigint 处理：`u64/usize` 等重映射为 TS `number`（运行期 serde_wasm_bindgen 给的就是
//! Number）——独立导出路径没有 tauri-specta 的 `dangerously_cast_bigints_to_number()`
//! 开关，照它内部实现（lang/js_ts.rs）用 `specta_util::Remapper` 复刻。

#![cfg(all(not(target_family = "wasm"), feature = "specta"))]

use std::borrow::Cow;

use specta::datatype::{DataType, Primitive};
use specta::{Format, FormatError, Type, Types};
use specta_util::Remapper;
use swarmdrop_web::{
    ConnectionJson, OfferJson, PendingPairingJson, RelayInfoJson, WebError, WebTransferEvent,
};

/// serde 形状（tagged enum / rename）+ bigint→number 重映射。
struct WebFormat(Remapper);

impl WebFormat {
    fn new() -> Self {
        let number = <specta_typescript::Number as Type>::definition(&mut Types::default());
        let remapper = Remapper::new()
            .rule(DataType::Primitive(Primitive::usize), number.clone())
            .rule(DataType::Primitive(Primitive::isize), number.clone())
            .rule(DataType::Primitive(Primitive::u64), number.clone())
            .rule(DataType::Primitive(Primitive::i64), number.clone())
            .rule(DataType::Primitive(Primitive::u128), number.clone())
            .rule(DataType::Primitive(Primitive::i128), number.clone())
            .rule(
                <specta_typescript::BigInt as Type>::definition(&mut Types::default()),
                number,
            );
        Self(remapper)
    }
}

impl Format for WebFormat {
    fn map_types(&self, types: &Types) -> Result<Cow<'_, Types>, FormatError> {
        let t = specta_serde::Format.map_types(types)?;
        Ok(Cow::Owned(self.0.remap_types(t.into_owned())))
    }

    fn map_type(&self, types: &Types, dt: &DataType) -> Result<Cow<'_, DataType>, FormatError> {
        let d = specta_serde::Format.map_type(types, dt)?;
        Ok(Cow::Owned(self.0.remap_dt(d.into_owned())))
    }
}

#[test]
fn export_bindings() {
    let types = Types::default()
        .register::<WebTransferEvent>()
        .register::<OfferJson>()
        .register::<PendingPairingJson>()
        .register::<ConnectionJson>()
        .register::<RelayInfoJson>()
        .register::<WebError>();

    specta_typescript::Typescript::default()
        .header(
            "// 由 `cargo test -p swarmdrop-web --features specta --test specta_export` 生成，\
             勿手改。\n// 形状与运行期 serde_wasm_bindgen 序列化一致（u64 等已映射为 number）。\n",
        )
        .export_to(
            concat!(env!("CARGO_MANIFEST_DIR"), "/bindings/bindings.ts"),
            &types,
            WebFormat::new(),
        )
        .expect("导出 bindings.ts 失败");
}
