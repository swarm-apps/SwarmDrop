//! 云发布只读取已接收并校验的本机暂存，不接管 P2P 随机写。
use crate::{CloudStorageError, PublishIntent, StagedFile, StorageResult};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

pub(crate) async fn verify_staging(
    staged: &StagedFile,
    intent: &PublishIntent,
) -> StorageResult<tokio::fs::File> {
    let mut file = tokio::fs::File::open(&staged.path)
        .await
        .map_err(|_| CloudStorageError::StagingChanged)?;
    if file
        .metadata()
        .await
        .map_err(|_| CloudStorageError::StagingChanged)?
        .len()
        != intent.size
    {
        return Err(CloudStorageError::StagingChanged);
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0; 256 * 1024];
    loop {
        let length = file
            .read(&mut buffer)
            .await
            .map_err(|_| CloudStorageError::StagingChanged)?;
        if length == 0 {
            break;
        }
        hasher.update(&buffer[..length]);
    }
    if hasher.finalize().to_hex().as_str() != intent.checksum {
        return Err(CloudStorageError::StagingChanged);
    }
    file.seek(std::io::SeekFrom::Start(0))
        .await
        .map_err(|_| CloudStorageError::StagingChanged)?;
    Ok(file)
}
