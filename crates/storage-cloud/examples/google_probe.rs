//! 真实 BYO Drive 契约探针：只生成测试内容，不读取用户文件或输出凭证。
use std::{path::PathBuf, sync::Arc, time::Instant};
use swarmdrop_cloud_auth::{CloudAccountManager, GoogleProvider, JsonCredentialStore};
use swarmdrop_host::ReceiveFileIdentity;
use swarmdrop_storage_cloud::{CloudPublisher, GoogleDrivePublisher, PublishIntent, StagedFile};
use tokio::io::AsyncWriteExt;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("缺少探针数据目录")?);
    let account_id = std::env::args().nth(2).ok_or("缺少账户标识")?;
    let accounts = CloudAccountManager::new(
        Arc::new(JsonCredentialStore::new(root.clone())),
        vec![Arc::new(GoogleProvider::new()?)],
    )
    .await?;
    let publisher = GoogleDrivePublisher::new(accounts, root.clone())?;
    let path = root.join("probe-payload.bin");
    let mut file = tokio::fs::File::create(&path).await?;
    let block = vec![0x5a; 256 * 1024];
    let mut hash = blake3::Hasher::new();
    for _ in 0..68 {
        file.write_all(&block).await?;
        hash.update(&block);
    }
    file.sync_all().await?;
    drop(file);
    let size = 17 * 1024 * 1024;
    let intent = PublishIntent {
        provider: swarmdrop_host::CloudProvider::GoogleDrive,
        account_id,
        root: None,
        identity: ReceiveFileIdentity {
            session_id: uuid::Uuid::new_v4().to_string(),
            file_id: 0,
            sender_device_id: "probe-full-sender-device-identity".into(),
            receiver_device_id: "probe-full-receiver-device-identity".into(),
        },
        relative_path: format!("契约探针/{}/完整性验证.bin", uuid::Uuid::new_v4()),
        original_name: "完整性验证.bin".into(),
        size,
        checksum: hash.finalize().to_hex().to_string(),
    };
    let started = Instant::now();
    let object = publisher
        .publish(
            StagedFile { path: path.clone() },
            intent.clone(),
            Arc::new(|progress| {
                println!(
                    "云上传：{}/{}，{} 字节/秒",
                    progress.uploaded_bytes, progress.total_bytes, progress.bytes_per_second
                )
            }),
        )
        .await?;
    let upload_seconds = started.elapsed().as_secs_f64();
    let resumed = publisher
        .publish(
            StagedFile { path: path.clone() },
            intent.clone(),
            Arc::new(|_| {}),
        )
        .await?;
    if resumed.object_id != object.object_id {
        return Err("同一接收记录未复用对象".into());
    }
    let mut repeated = intent.clone();
    repeated.identity.session_id = uuid::Uuid::new_v4().to_string();
    let reused = publisher
        .publish(StagedFile { path }, repeated.clone(), Arc::new(|_| {}))
        .await?;
    if reused.object_id != object.object_id {
        return Err("同路径同内容未复用对象".into());
    }
    let _ = publisher.open_url(&object).await?;
    let result = serde_json::json!({"object":object,"uploadSeconds":upload_seconds,"size":size,"receiptRetry":true,"sameDeviceReuse":true,"metadataQuery":true});
    tokio::fs::write(
        root.join("drive-probe-result.json"),
        serde_json::to_vec_pretty(&result)?,
    )
    .await?;
    publisher.confirm_committed(&intent).await?;
    publisher.confirm_committed(&repeated).await?;
    println!("Drive 探针完成：目录、属性查询、文件 ID 查询、分块上传及重复接收复用通过。");
    Ok(())
}
