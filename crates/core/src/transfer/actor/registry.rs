//! 传输 actor 注册表。
//!
//! SenderActor / ReceiverActor 的运行时生命周期集中在这里管理：创建、替换、
//! 移除、取消和 epoch 准入。Coordinator 负责持久化状态机，ActorRegistry 负责
//! 内存 actor 的唯一性，二者共同避免旧 actor 在恢复后覆盖新状态。
//!
//! send / receive 两侧逻辑同构，仅 actor 类型不同：泛型 [`Registered<A>`] + 自由
//! helper（`insert_actor` / `get_actor` / `remove_actor` / `remove_actor_if_epoch`）消复制，
//! `ActorRegistry` 的两侧方法只是按 actor 类型分派到对应 DashMap 的薄包装。

use std::sync::Arc;

use dashmap::DashMap;
use uuid::Uuid;

use crate::transfer::actor::receiver::ReceiverActor;
use crate::transfer::actor::sender::SenderActor;
use crate::transfer::epoch::EpochGuard;

/// 可被注册表取消的 actor（epoch 替换 / 移除时调用）。
pub trait Cancellable {
    fn cancel(&self);
}

impl Cancellable for SenderActor {
    fn cancel(&self) {
        // inherent `cancel` 优先于 trait 方法（Rust 方法解析规则），不会递归。
        self.cancel();
    }
}

impl Cancellable for ReceiverActor {
    fn cancel(&self) {
        self.cancel();
    }
}

/// 注册表中的一条 actor 记录：epoch + actor 句柄。
struct Registered<A> {
    epoch: i64,
    actor: Arc<A>,
}

#[derive(Clone)]
pub struct ActorRegistry {
    send: Arc<DashMap<Uuid, Registered<SenderActor>>>,
    receive: Arc<DashMap<Uuid, Registered<ReceiverActor>>>,
}

impl ActorRegistry {
    pub fn new() -> Self {
        Self {
            send: Arc::new(DashMap::new()),
            receive: Arc::new(DashMap::new()),
        }
    }

    pub fn insert_send(&self, session_id: Uuid, epoch: i64, actor: Arc<SenderActor>) -> bool {
        insert_actor(&self.send, session_id, epoch, actor)
    }

    pub fn insert_receive(&self, session_id: Uuid, epoch: i64, actor: Arc<ReceiverActor>) -> bool {
        insert_actor(&self.receive, session_id, epoch, actor)
    }

    pub fn get_send(&self, session_id: &Uuid) -> Option<Arc<SenderActor>> {
        get_actor(&self.send, session_id)
    }

    pub fn get_receive(&self, session_id: &Uuid) -> Option<Arc<ReceiverActor>> {
        get_actor(&self.receive, session_id)
    }

    pub fn receive_epoch(&self, session_id: &Uuid) -> Option<i64> {
        self.receive.get(session_id).map(|r| r.epoch)
    }

    pub fn remove_send(&self, session_id: &Uuid) -> Option<Arc<SenderActor>> {
        remove_actor(&self.send, session_id)
    }

    pub fn remove_receive(&self, session_id: &Uuid) -> Option<Arc<ReceiverActor>> {
        remove_actor(&self.receive, session_id)
    }

    /// 仅当当前 epoch 匹配才移除（发送完成 / 接收后台任务结束的 teardown 用）。
    /// 防止旧 epoch actor 的收尾任务误删 resume 后注册的新 epoch actor。
    pub fn remove_send_if_epoch(&self, session_id: &Uuid, epoch: i64) -> Option<Arc<SenderActor>> {
        remove_actor_if_epoch(&self.send, session_id, epoch)
    }

    pub fn remove_receive_if_epoch(
        &self,
        session_id: &Uuid,
        epoch: i64,
    ) -> Option<Arc<ReceiverActor>> {
        remove_actor_if_epoch(&self.receive, session_id, epoch)
    }

    pub fn idle_send_ids(&self, max_idle_ms: u64) -> Vec<Uuid> {
        self.send
            .iter()
            .filter(|entry| entry.value().actor.idle_ms() > max_idle_ms)
            .map(|entry| *entry.key())
            .collect()
    }
}

/// 插入新 epoch actor：同 / 旧 epoch 拒绝并取消传入 actor；更高 epoch 取消并替换旧 actor。
fn insert_actor<A: Cancellable>(
    map: &DashMap<Uuid, Registered<A>>,
    session_id: Uuid,
    epoch: i64,
    actor: Arc<A>,
) -> bool {
    if let Some(existing) = map.get(&session_id)
        && !EpochGuard::is_newer(epoch, existing.epoch)
    {
        // 同 / 旧 epoch（非严格更新）拒绝并取消传入 actor。
        actor.cancel();
        return false;
    }
    if let Some((_, old)) = map.remove(&session_id) {
        old.actor.cancel();
    }
    map.insert(session_id, Registered { epoch, actor });
    true
}

fn get_actor<A>(map: &DashMap<Uuid, Registered<A>>, session_id: &Uuid) -> Option<Arc<A>> {
    map.get(session_id).map(|r| Arc::clone(&r.actor))
}

fn remove_actor<A>(map: &DashMap<Uuid, Registered<A>>, session_id: &Uuid) -> Option<Arc<A>> {
    map.remove(session_id).map(|(_, entry)| entry.actor)
}

fn remove_actor_if_epoch<A>(
    map: &DashMap<Uuid, Registered<A>>,
    session_id: &Uuid,
    epoch: i64,
) -> Option<Arc<A>> {
    let current_epoch = map.get(session_id).map(|entry| entry.epoch)?;
    if current_epoch != epoch {
        return None;
    }
    remove_actor(map, session_id)
}

impl Default for ActorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use swarm_p2p_core::libp2p::identity::Keypair;

    use super::*;
    use crate::host::{CoreAppPaths, MemoryHost};
    use crate::transfer::manager::PreparedFile;

    fn send_actor() -> Arc<SenderActor> {
        let peer_id = Keypair::generate_ed25519().public().to_peer_id();
        let base = std::env::temp_dir();
        let host = Arc::new(MemoryHost::new(CoreAppPaths {
            data_dir: base.clone(),
            cache_dir: base.clone(),
            temp_dir: base.clone(),
            log_dir: base,
        }));
        Arc::new(SenderActor::new(
            Uuid::new_v4(),
            peer_id,
            Vec::<PreparedFile>::new(),
            &[7; 32],
            host.clone(),
            host,
        ))
    }

    #[test]
    fn rejects_same_or_older_epoch_actor() {
        let registry = ActorRegistry::new();
        let session_id = Uuid::new_v4();
        let current = send_actor();
        let stale = send_actor();

        assert!(registry.insert_send(session_id, 2, current.clone()));
        assert!(!registry.insert_send(session_id, 2, stale.clone()));

        assert!(stale.cancel_token().is_cancelled());
        let stored = registry.get_send(&session_id).expect("stored actor");
        assert!(Arc::ptr_eq(&stored, &current));
    }

    #[test]
    fn newer_epoch_replaces_and_cancels_old_actor() {
        let registry = ActorRegistry::new();
        let session_id = Uuid::new_v4();
        let old = send_actor();
        let new = send_actor();

        assert!(registry.insert_send(session_id, 1, old.clone()));
        assert!(registry.insert_send(session_id, 2, new.clone()));

        assert!(old.cancel_token().is_cancelled());
        let stored = registry.get_send(&session_id).expect("stored actor");
        assert!(Arc::ptr_eq(&stored, &new));
    }

    #[test]
    fn remove_send_if_epoch_only_removes_matching_epoch() {
        let registry = ActorRegistry::new();
        let session_id = Uuid::new_v4();
        let new = send_actor();
        registry.insert_send(session_id, 2, new.clone());

        // 旧 epoch 收尾任务（epoch=1）不得误删 resume 后的新 epoch actor（epoch=2）。
        assert!(registry.remove_send_if_epoch(&session_id, 1).is_none());
        assert!(registry.get_send(&session_id).is_some());

        // 匹配 epoch 才真正移除。
        let removed = registry.remove_send_if_epoch(&session_id, 2).expect("removed");
        assert!(Arc::ptr_eq(&removed, &new));
        assert!(registry.get_send(&session_id).is_none());
    }
}
