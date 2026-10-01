//! 原生上传状态的持久 IO，写入必须先同步再原子替换。
use crate::{CloudStorageError, StorageResult};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) async fn load_json<T: DeserializeOwned>(path: PathBuf) -> StorageResult<Option<T>> {
    match tokio::fs::read(path).await {
        Ok(data) => serde_json::from_slice(&data)
            .map(Some)
            .map_err(|_| CloudStorageError::Checkpoint),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(CloudStorageError::Checkpoint),
    }
}
pub(crate) async fn save_json<T: Serialize>(path: PathBuf, value: &T) -> StorageResult<()> {
    let data = serde_json::to_vec(value).map_err(|_| CloudStorageError::Checkpoint)?;
    tokio::task::spawn_blocking(move || {
        let dir = path.parent().ok_or(CloudStorageError::Checkpoint)?;
        std::fs::create_dir_all(dir).map_err(|_| CloudStorageError::Checkpoint)?;
        let mut file = tempfile::Builder::new()
            .prefix(".pending-")
            .tempfile_in(dir)
            .map_err(|_| CloudStorageError::Checkpoint)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| CloudStorageError::Checkpoint)?;
        }
        file.write_all(&data)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|_| CloudStorageError::Checkpoint)?;
        file.persist(&path)
            .map_err(|_| CloudStorageError::Checkpoint)?;
        sync_directory(dir)
    })
    .await
    .map_err(|_| CloudStorageError::Checkpoint)?
}
pub(crate) async fn remove_file(path: PathBuf) -> StorageResult<()> {
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {
            let dir = path
                .parent()
                .ok_or(CloudStorageError::Checkpoint)?
                .to_owned();
            tokio::task::spawn_blocking(move || sync_directory(&dir))
                .await
                .map_err(|_| CloudStorageError::Checkpoint)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(CloudStorageError::Checkpoint),
    }
}
fn sync_directory(dir: &Path) -> StorageResult<()> {
    #[cfg(unix)]
    std::fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| CloudStorageError::Checkpoint)?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}
