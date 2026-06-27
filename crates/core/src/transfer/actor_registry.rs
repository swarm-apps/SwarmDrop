//! 传输 actor 注册表。
//!
//! SenderActor / ReceiverActor 的运行时生命周期集中在这里管理：创建、替换、
//! 移除、取消和 epoch 准入。Coordinator 负责持久化状态机，ActorRegistry 负责
//! 内存 actor 的唯一性，二者共同避免旧 actor 在恢复后覆盖新状态。

use std::sync::Arc;

use dashmap::DashMap;
use uuid::Uuid;

use crate::transfer::receiver::ReceiveSession;
use crate::transfer::sender::SendSession;

#[derive(Clone)]
pub struct ActorRegistry {
    send: Arc<DashMap<Uuid, RegisteredSendActor>>,
    receive: Arc<DashMap<Uuid, RegisteredReceiveActor>>,
}

#[derive(Clone)]
struct RegisteredSendActor {
    epoch: i64,
    actor: Arc<SendSession>,
}

#[derive(Clone)]
struct RegisteredReceiveActor {
    epoch: i64,
    actor: Arc<ReceiveSession>,
}

impl ActorRegistry {
    pub fn new() -> Self {
        Self {
            send: Arc::new(DashMap::new()),
            receive: Arc::new(DashMap::new()),
        }
    }

    pub fn insert_send(&self, session_id: Uuid, epoch: i64, actor: Arc<SendSession>) -> bool {
        if let Some(existing) = self.send.get(&session_id)
            && existing.epoch >= epoch
        {
            actor.cancel();
            return false;
        }
        if let Some((_, old)) = self.send.remove(&session_id) {
            old.actor.cancel();
        }
        self.send
            .insert(session_id, RegisteredSendActor { epoch, actor });
        true
    }

    pub fn insert_receive(&self, session_id: Uuid, epoch: i64, actor: Arc<ReceiveSession>) -> bool {
        if let Some(existing) = self.receive.get(&session_id)
            && existing.epoch >= epoch
        {
            actor.cancel();
            return false;
        }
        if let Some((_, old)) = self.receive.remove(&session_id) {
            old.actor.cancel();
        }
        self.receive
            .insert(session_id, RegisteredReceiveActor { epoch, actor });
        true
    }

    pub fn get_send(&self, session_id: &Uuid) -> Option<Arc<SendSession>> {
        self.send.get(session_id).map(|r| Arc::clone(&r.actor))
    }

    pub fn get_receive(&self, session_id: &Uuid) -> Option<Arc<ReceiveSession>> {
        self.receive.get(session_id).map(|r| Arc::clone(&r.actor))
    }

    pub fn receive_epoch(&self, session_id: &Uuid) -> Option<i64> {
        self.receive.get(session_id).map(|r| r.epoch)
    }

    pub fn remove_send(&self, session_id: &Uuid) -> Option<Arc<SendSession>> {
        self.send.remove(session_id).map(|(_, entry)| entry.actor)
    }

    pub fn remove_receive(&self, session_id: &Uuid) -> Option<Arc<ReceiveSession>> {
        self.receive
            .remove(session_id)
            .map(|(_, entry)| entry.actor)
    }

    pub fn remove_receive_if_epoch(
        &self,
        session_id: &Uuid,
        epoch: i64,
    ) -> Option<Arc<ReceiveSession>> {
        let current_epoch = self.receive.get(session_id).map(|entry| entry.epoch)?;
        if current_epoch != epoch {
            return None;
        }
        self.remove_receive(session_id)
    }

    pub fn idle_send_ids(&self, max_idle_ms: u64) -> Vec<Uuid> {
        self.send
            .iter()
            .filter(|entry| entry.value().actor.idle_ms() > max_idle_ms)
            .map(|entry| *entry.key())
            .collect()
    }
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

    fn send_actor() -> Arc<SendSession> {
        let peer_id = Keypair::generate_ed25519().public().to_peer_id();
        let base = std::env::temp_dir();
        let host = Arc::new(MemoryHost::new(CoreAppPaths {
            data_dir: base.clone(),
            cache_dir: base.clone(),
            temp_dir: base.clone(),
            log_dir: base,
        }));
        Arc::new(SendSession::new(
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
}
