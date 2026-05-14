//! Android 移动端插件桥接
//!
//! 通过 Tauri Plugin Builder 注册 Kotlin 插件到运行时，
//! 实际业务逻辑在 UpdaterPlugin.kt 中实现。

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "com.yexiyue.swarmdrop";

/// Android 更新插件句柄（仅 Android 编译）
#[cfg(target_os = "android")]
pub struct UpdaterPlugin<R: Runtime>(tauri::plugin::PluginHandle<R>);

#[cfg(target_os = "android")]
impl<R: Runtime> UpdaterPlugin<R> {
    pub fn install_update(&self, url: String, is_force: bool) -> crate::AppResult<()> {
        #[derive(serde::Serialize)]
        struct Payload {
            url: String,
            #[serde(rename = "isForce")]
            is_force: bool,
        }

        self.0
            .run_mobile_plugin("installUpdate", Payload { url, is_force })
            .map_err(|e| crate::AppError::Network(e.to_string()))
    }
}

/// 构建 Android 更新插件
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("android-updater")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "UpdaterPlugin")?;
                app.manage(UpdaterPlugin(handle));
            }

            #[cfg(not(target_os = "android"))]
            let _ = (app, api);

            Ok(())
        })
        .build()
}
