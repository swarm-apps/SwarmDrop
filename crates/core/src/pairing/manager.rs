use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use swarm_p2p_core::libp2p::{kad::Record, Multiaddr, PeerId};

use super::code::{OnlineRecord, PairingCodeInfo, ShareCodeRecord};
use super::dht_key;
use crate::device::{OsInfo, PairedDeviceInfo};
use crate::protocol::{
    AppNetClient, AppRequest, AppResponse, PairingMethod, PairingRequest, PairingResponse,
};
use crate::{AppError, AppResult};

/// 入站配对请求缓存（事件循环写入，handle_pairing_request 消费）
struct PendingInbound {
    peer_id: PeerId,
    os_info: OsInfo,
}

/// 配对管理器
///
/// 管理配对码生成/查询、DHT 在线宣告、配对请求/响应处理，
/// 以及已配对设备的增删查。
///
/// 本身不含 Arc，需要共享时由使用方包裹 `Arc<PairingManager>`。
pub struct PairingManager {
    client: AppNetClient,
    peer_id: PeerId,
    /// 当前活跃的配对码（单例，同一时刻最多一个）
    active_code: Mutex<Option<PairingCodeInfo>>,
    /// 已配对设备（与 DeviceManager 共享读取）
    paired_devices: Arc<DashMap<PeerId, PairedDeviceInfo>>,
    /// 入站请求缓存，handle_pairing_request 时取出
    pending_inbound: DashMap<u64, PendingInbound>,
    /// get_device_info 查询时缓存对端 OsInfo，request_pairing 成功后使用
    discovered_peers: DashMap<PeerId, OsInfo>,
}

impl PairingManager {
    pub fn new(
        client: AppNetClient,
        peer_id: PeerId,
        paired_devices: Arc<DashMap<PeerId, PairedDeviceInfo>>,
    ) -> Self {
        Self {
            client,
            peer_id,
            active_code: Mutex::new(None),
            paired_devices,
            pending_inbound: DashMap::new(),
            discovered_peers: DashMap::new(),
        }
    }

    /// 序列化数据并发布到 DHT
    async fn put_json_record(
        &self,
        key: swarm_p2p_core::libp2p::kad::RecordKey,
        data: &impl serde::Serialize,
        ttl_secs: u64,
    ) -> AppResult<()> {
        self.client
            .put_record(Record {
                key,
                value: serde_json::to_vec(data)?,
                publisher: Some(self.peer_id),
                expires: Some(Instant::now() + Duration::from_secs(ttl_secs)),
            })
            .await?;
        Ok(())
    }

    // === DHT 在线宣告 ===

    /// 宣布上线：将本节点的可达地址发布到 DHT
    pub async fn announce_online(&self) -> AppResult<()> {
        let addrs = self.client.get_addrs().await?;
        let record_data = OnlineRecord {
            os_info: OsInfo::default(),
            listen_addrs: addrs,
            timestamp: chrono::Utc::now().timestamp(),
        };
        self.put_json_record(
            dht_key::online_key(&self.peer_id.to_bytes()),
            &record_data,
            300,
        )
        .await
    }

    /// 启动后检查已配对设备是否在线
    ///
    /// 在 DHT bootstrap 完成后调用。对每个已配对设备查询其在线记录，
    /// 找到则将地址注册到地址簿，使后续传输可直接 dial，无需重新配对。
    pub async fn check_paired_online(&self) {
        let paired = self.get_paired_devices();
        if paired.is_empty() {
            return;
        }

        tracing::info!("检查 {} 个已配对设备的在线状态", paired.len());

        for device in paired {
            let key = dht_key::online_key(&device.peer_id.to_bytes());
            match self.client.get_record(key).await {
                Ok(result) => {
                    let record = result.record;
                    // 跳过已过期记录
                    if record.expires.map(|e| e < Instant::now()).unwrap_or(false) {
                        continue;
                    }
                    if let Ok(online_record) = serde_json::from_slice::<OnlineRecord>(&record.value)
                    {
                        if online_record.listen_addrs.is_empty() {
                            continue;
                        }
                        if let Err(e) = self
                            .client
                            .add_peer_addrs(device.peer_id, online_record.listen_addrs)
                            .await
                        {
                            tracing::warn!("注册 {} 地址失败: {}", device.peer_id, e);
                            continue;
                        }
                        // 主动 dial：连接成功后触发 PeerConnected 事件，
                        // 事件循环推送 devices-changed，前端自动更新在线状态
                        if let Err(e) = self.client.dial(device.peer_id).await {
                            tracing::warn!("拨号 {} 失败: {}", device.peer_id, e);
                        } else {
                            tracing::info!("已向已配对设备 {} 发起重连", device.peer_id);
                        }
                    }
                }
                Err(_) => {
                    // 设备离线或 DHT 查询失败，正常现象，静默忽略
                }
            }
        }
    }

    /// 宣布下线：从 DHT 移除在线记录
    pub async fn announce_offline(&self) -> AppResult<()> {
        self.client
            .remove_record(dht_key::online_key(&self.peer_id.to_bytes()))
            .await?;
        Ok(())
    }

    // === 配对码管理 ===

    pub async fn generate_code(&self, expires_in_secs: u64) -> AppResult<PairingCodeInfo> {
        let code_info = PairingCodeInfo::generate(expires_in_secs);

        // 获取当前监听地址，嵌入 DHT Record，供对方 dial 时使用
        let addrs = self.client.get_addrs().await?;
        let mut record_data = ShareCodeRecord::from(&code_info);
        record_data.listen_addrs = addrs;

        self.put_json_record(
            dht_key::share_code_key(&code_info.code),
            &record_data,
            expires_in_secs,
        )
        .await?;

        // 覆盖旧码（旧 DHT 记录靠 TTL 自然过期，无需显式删除）
        *self.active_code.lock().unwrap() = Some(code_info.clone());

        Ok(code_info)
    }

    // === 配对流程 ===

    /// 查询配对码对应的设备信息，并缓存 OsInfo 供后续 request_pairing 使用
    pub async fn get_device_info(&self, code: &str) -> AppResult<(PeerId, ShareCodeRecord)> {
        let record = self
            .client
            .get_record(dht_key::share_code_key(code))
            .await?
            .record;

        if let Some(expires) = record.expires {
            if expires < Instant::now() {
                return Err(AppError::ExpiredCode);
            }
        }

        let peer_id = record.publisher.ok_or(AppError::InvalidCode)?;
        let share_record = serde_json::from_slice::<ShareCodeRecord>(&record.value)?;

        // 将记录中的地址注册到 Swarm 地址簿，确保后续 dial 能找到对方
        if !share_record.listen_addrs.is_empty() {
            self.client
                .add_peer_addrs(peer_id, share_record.listen_addrs.clone())
                .await?;
        }

        // 缓存对端 OsInfo，request_pairing 成功后用于构造 PairedDeviceInfo
        self.discovered_peers
            .insert(peer_id, share_record.os_info.clone());

        Ok((peer_id, share_record))
    }

    /// 发起配对请求
    ///
    /// 返回 `(PairingResponse, Option<PairedDeviceInfo>)`：
    /// - 对方接受 → 自动添加到已配对设备，返回 `Some(info)`
    /// - 对方拒绝 → 返回 `None`
    pub async fn request_pairing(
        &self,
        peer_id: PeerId,
        method: PairingMethod,
        addrs: Option<Vec<Multiaddr>>,
    ) -> AppResult<(PairingResponse, Option<PairedDeviceInfo>)> {
        if let Some(addrs) = addrs.filter(|a| !a.is_empty()) {
            self.client.add_peer_addrs(peer_id, addrs).await?;
        }

        self.client.dial(peer_id).await?;

        let res = self
            .client
            .send_request(
                peer_id,
                AppRequest::Pairing(PairingRequest {
                    os_info: OsInfo::default(),
                    method,
                    timestamp: chrono::Utc::now().timestamp(),
                }),
            )
            .await?;

        match res {
            AppResponse::Pairing(PairingResponse::Success) => {
                let os_info = self
                    .discovered_peers
                    .remove(&peer_id)
                    .map(|(_, info)| info)
                    .unwrap_or_else(|| OsInfo::unknown_from_peer_id(&peer_id));

                let info = PairedDeviceInfo {
                    peer_id,
                    os_info,
                    paired_at: chrono::Utc::now().timestamp_millis(),
                };
                self.paired_devices.insert(peer_id, info.clone());

                Ok((PairingResponse::Success, Some(info)))
            }
            AppResponse::Pairing(resp) => Ok((resp, None)),
            other => Err(crate::AppError::Network(format!(
                "意外的响应类型: {other:?}"
            ))),
        }
    }

    /// 处理收到的配对请求并发送响应
    ///
    /// - `Code` 模式：验证配对码存在且未过期，验证通过后消耗该配对码
    /// - `Direct` 模式：局域网直连，由用户在 UI 确认授权，无需配对码
    ///
    /// 返回 `Some(PairedDeviceInfo)` 表示配对已接受并添加到已配对设备
    pub async fn handle_pairing_request(
        &self,
        pending_id: u64,
        method: &PairingMethod,
        response: PairingResponse,
    ) -> AppResult<Option<PairedDeviceInfo>> {
        // 仅在接受时验证并消耗配对码；拒绝时直接发响应，无需验证
        if let PairingMethod::Code { code } = method {
            if matches!(response, PairingResponse::Success) {
                let mut guard = self.active_code.lock().unwrap();
                let info = guard.as_ref().ok_or(AppError::InvalidCode)?;
                if &info.code != code {
                    return Err(AppError::InvalidCode);
                }
                if info.is_expired() {
                    return Err(AppError::ExpiredCode);
                }
                *guard = None;
                // guard 在此处 drop，锁在 await 之前释放
            }
        }

        let accepted = matches!(response, PairingResponse::Success);

        self.client
            .send_response(pending_id, AppResponse::Pairing(response))
            .await?;

        // 拒绝或缓存不存在 → 清理后返回 None
        let Some((_, pending)) = accepted
            .then(|| self.pending_inbound.remove(&pending_id))
            .flatten()
        else {
            self.pending_inbound.remove(&pending_id);
            return Ok(None);
        };

        // 接受配对 → 构造 PairedDeviceInfo 并存储
        let info = PairedDeviceInfo {
            peer_id: pending.peer_id,
            os_info: pending.os_info,
            paired_at: chrono::Utc::now().timestamp_millis(),
        };
        self.paired_devices.insert(info.peer_id, info.clone());
        Ok(Some(info))
    }

    // === 入站请求缓存 ===

    /// 缓存入站配对请求上下文（事件循环调用）
    ///
    /// 存储 peer_id 和 os_info，供 [`handle_pairing_request`] 接受时构造 PairedDeviceInfo。
    pub fn cache_inbound_request(
        &self,
        peer_id: PeerId,
        pending_id: u64,
        request: &PairingRequest,
    ) {
        self.pending_inbound.insert(
            pending_id,
            PendingInbound {
                peer_id,
                os_info: request.os_info.clone(),
            },
        );
    }

    // === 已配对设备管理 ===

    pub fn is_paired(&self, peer_id: &PeerId) -> bool {
        self.paired_devices.contains_key(peer_id)
    }

    pub fn add_paired_device(&self, info: PairedDeviceInfo) {
        self.paired_devices.insert(info.peer_id, info);
    }

    pub fn remove_paired_device(&self, peer_id: &PeerId) -> Option<PairedDeviceInfo> {
        self.paired_devices.remove(peer_id).map(|(_, v)| v)
    }

    pub fn get_paired_devices(&self) -> Vec<PairedDeviceInfo> {
        self.paired_devices
            .iter()
            .map(|e| e.value().clone())
            .collect()
    }
}
