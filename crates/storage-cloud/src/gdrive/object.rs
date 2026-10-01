//! Drive 对象的来源属性、路径规则和内容身份核验。
use crate::{CloudStorageError, PublishIntent, StorageResult};
use serde::Deserialize;
use std::collections::BTreeMap;
use swarmdrop_host::{CloudObjectRef, CloudProvider};
pub(super) const FILE_FIELDS: &str = "id,name,size,parents,appProperties,trashed";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DriveFile {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) size: Option<String>,
    #[serde(default)]
    pub(super) parents: Vec<String>,
    #[serde(default)]
    pub(super) app_properties: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) trashed: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DriveFiles {
    pub(super) files: Vec<DriveFile>,
    pub(super) next_page_token: Option<String>,
}
pub(super) fn validate_id(id: &str) -> StorageResult<()> {
    if id.is_empty()
        || id.len() > 100
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(CloudStorageError::Configuration);
    }
    Ok(())
}
pub(super) fn escape_query(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}
pub(super) fn clean_path(path: &str) -> StorageResult<Vec<String>> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return Err(CloudStorageError::InvalidPath);
    }
    path.replace('\\', "/")
        .split('/')
        .map(|part| {
            if part.is_empty()
                || part == "."
                || part == ".."
                || part.len() > 255
                || part.chars().any(char::is_control)
            {
                return Err(CloudStorageError::InvalidPath);
            }
            Ok(part.to_owned())
        })
        .collect()
}
pub(super) fn conflict_name(name: &str, suffix: &str) -> String {
    let (base, extension) = name
        .rsplit_once('.')
        .filter(|(base, _)| !base.is_empty())
        .unwrap_or((name, ""));
    let extension = if extension.is_empty() {
        String::new()
    } else {
        format!(".{extension}")
    };
    format!("{base} ({suffix}){extension}")
}
pub(super) fn properties(intent: &PublishIntent) -> StorageResult<BTreeMap<String, String>> {
    let mut properties = BTreeMap::from([
        ("receipt_key".into(), intent.receipt_key()),
        ("target_path".into(), intent.path_key()),
        (
            "sender_device".into(),
            intent.identity.sender_device_id.clone(),
        ),
        (
            "receiver_device".into(),
            intent.identity.receiver_device_id.clone(),
        ),
        ("session_id".into(), intent.identity.session_id.clone()),
        ("file_id".into(), intent.identity.file_id.to_string()),
        ("blake3".into(), intent.checksum.clone()),
    ]);
    // Drive 每个属性键和值总共最多 124 字节；UTF-8 文件名按字符边界拆分，不截断来源信息。
    let mut fragment = String::new();
    let mut index = 0;
    for character in intent.original_name.chars() {
        if fragment.len() + character.len_utf8() > 100 {
            properties.insert(
                format!("original_name_{index}"),
                std::mem::take(&mut fragment),
            );
            index += 1;
        }
        fragment.push(character);
    }
    properties.insert(format!("original_name_{index}"), fragment);
    if properties.len() > 30
        || properties
            .iter()
            .any(|(key, value)| key.len() + value.len() > 124)
    {
        return Err(CloudStorageError::Configuration);
    }
    Ok(properties)
}
pub(super) fn verify_file(
    file: &DriveFile,
    intent: &PublishIntent,
    parent: &str,
) -> StorageResult<()> {
    if file.trashed
        || file
            .size
            .as_deref()
            .and_then(|size| size.parse::<u64>().ok())
            != Some(intent.size)
        || !file.parents.iter().any(|id| id == parent)
        || file.app_properties.get("blake3") != Some(&intent.checksum)
        || file.app_properties.get("target_path") != Some(&intent.path_key())
        || file.app_properties.get("receiver_device") != Some(&intent.identity.receiver_device_id)
    {
        return Err(CloudStorageError::Configuration);
    }
    Ok(())
}
pub(super) fn object_ref(intent: &PublishIntent, file: DriveFile) -> CloudObjectRef {
    let prefix = intent
        .relative_path
        .rsplit_once('/')
        .map_or(String::new(), |(dir, _)| format!("{dir}/"));
    CloudObjectRef {
        provider: CloudProvider::GoogleDrive,
        account_id: intent.account_id.clone(),
        object_id: file.id,
        display_path: format!(
            "SwarmDrop/{}/{}{}",
            intent.identity.receiver_device_id, prefix, file.name
        ),
    }
}
