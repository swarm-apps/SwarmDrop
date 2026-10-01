//! 本机 BYO 探针：从用户提供的 Google 桌面客户端文件读取配置，不输出凭证。
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};
use swarmdrop_cloud_auth::{
    ClientCredentials, CloudAccountManager, GoogleProvider, JsonCredentialStore, ProviderId,
};

#[derive(Deserialize)]
struct DesktopClient {
    installed: ClientCredentials,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().ok_or("需要 Google Desktop client JSON 路径")?);
    let data_dir = PathBuf::from(args.next().ok_or("需要本机探针数据目录")?);
    let client: DesktopClient = serde_json::from_slice(&std::fs::read(path)?)?;
    let manager = CloudAccountManager::new(
        Arc::new(JsonCredentialStore::new(data_dir)),
        vec![Arc::new(GoogleProvider::new()?)],
    )
    .await?;
    let mut updates = manager.subscribe();
    let session = manager
        .start_connect(
            ProviderId::GoogleDrive,
            client.installed,
            "Google Drive 探针".into(),
            None,
        )
        .await?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&session.authorization_url)
        .status()?;
    #[cfg(not(target_os = "macos"))]
    return Err("该探针的浏览器启动适用于 macOS；其他系统请使用设置页".into());
    println!("系统浏览器已打开，请完成 Google 授权；凭证只写入本机探针目录。");
    loop {
        let update = updates.recv().await?;
        if update.session_id.as_deref() != Some(&session.id) {
            continue;
        }
        if let Some(error) = update.error {
            return Err(error.into());
        }
        if let Some(account) = update.account {
            println!("授权完成，账户标识：{}", account.id);
            break;
        }
    }
    Ok(())
}
