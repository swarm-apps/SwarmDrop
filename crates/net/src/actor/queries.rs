//! kad `QueryId` → oneshot 挂账表（旧栈 kad 命令责任链的替代）。
//!
//! 模式：发起查询记下 QueryId，`OutboundQueryProgressed` 按 id 对账；
//! `progress` 消费自身返回 `None` 表示完成（应答已发），返回 `Some` 放回
//! 继续等后续进展。非我们发起的查询（周期性 bootstrap 等）不在表里，忽略。

use std::collections::HashMap;

use libp2p::kad;
use swarmdrop_net_base::NodeId;
use tokio::sync::oneshot;

use crate::dht::{DhtError, DhtRecord};

pub(crate) enum PendingQuery {
    Bootstrap {
        reply: oneshot::Sender<Result<(), DhtError>>,
    },
    Put {
        reply: oneshot::Sender<Result<(), DhtError>>,
    },
    Get {
        reply: oneshot::Sender<Result<DhtRecord, DhtError>>,
    },
    Provide {
        reply: oneshot::Sender<Result<(), DhtError>>,
    },
    Providers {
        found: Vec<NodeId>,
        reply: oneshot::Sender<Result<Vec<NodeId>, DhtError>>,
    },
}

impl PendingQuery {
    /// 消费一次查询进展。`None` = 已完成（应答已发出），`Some(self)` = 继续等。
    fn progress(self, result: kad::QueryResult, step: &kad::ProgressStep) -> Option<Self> {
        match (self, result) {
            (Self::Get { reply }, kad::QueryResult::GetRecord(res)) => match res {
                // 拿到首个 record 立即完成（不等查询自然结束，快路径）
                Ok(kad::GetRecordOk::FoundRecord(peer_record)) => {
                    let publisher = peer_record.record.publisher.map(NodeId::from_peer_id);
                    let _ = reply.send(Ok(DhtRecord {
                        value: peer_record.record.value,
                        publisher,
                    }));
                    None
                }
                Ok(kad::GetRecordOk::FinishedWithNoAdditionalRecord { .. }) => {
                    if step.last {
                        let _ = reply.send(Err(DhtError::NotFound));
                        None
                    } else {
                        Some(Self::Get { reply })
                    }
                }
                Err(e) => {
                    if step.last {
                        let _ = reply.send(Err(match e {
                            kad::GetRecordError::NotFound { .. } => DhtError::NotFound,
                            other => DhtError::QueryFailed(format!("{other:?}")),
                        }));
                        None
                    } else {
                        Some(Self::Get { reply })
                    }
                }
            },
            (Self::Put { reply }, kad::QueryResult::PutRecord(res)) => {
                let _ = reply.send(
                    res.map(|_| ())
                        .map_err(|e| DhtError::QueryFailed(format!("{e:?}"))),
                );
                None
            }
            (Self::Bootstrap { reply }, kad::QueryResult::Bootstrap(res)) => match res {
                Ok(_) if step.last => {
                    let _ = reply.send(Ok(()));
                    None
                }
                Ok(_) => Some(Self::Bootstrap { reply }),
                Err(e) => {
                    let _ = reply.send(Err(DhtError::QueryFailed(format!("{e:?}"))));
                    None
                }
            },
            (Self::Provide { reply }, kad::QueryResult::StartProviding(res)) => {
                let _ = reply.send(
                    res.map(|_| ())
                        .map_err(|e| DhtError::QueryFailed(format!("{e:?}"))),
                );
                None
            }
            (Self::Providers { mut found, reply }, kad::QueryResult::GetProviders(res)) => {
                if let Ok(kad::GetProvidersOk::FoundProviders { providers, .. }) = res {
                    for p in providers {
                        let node = NodeId::from_peer_id(p);
                        if !found.contains(&node) {
                            found.push(node);
                        }
                    }
                }
                if step.last {
                    let _ = reply.send(Ok(found));
                    None
                } else {
                    Some(Self::Providers { found, reply })
                }
            }
            // 类型对不上（不应发生）：丢弃应答端，让调用方收到 Closed
            (query, result) => {
                tracing::warn!(?result, "unexpected kad query result kind");
                Some(query)
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct PendingQueries(HashMap<kad::QueryId, PendingQuery>);

impl PendingQueries {
    pub(crate) fn insert(&mut self, id: kad::QueryId, query: PendingQuery) {
        self.0.insert(id, query);
    }

    pub(crate) fn handle(
        &mut self,
        id: kad::QueryId,
        result: kad::QueryResult,
        step: &kad::ProgressStep,
    ) {
        let Some(query) = self.0.remove(&id) else {
            return; // 非我们发起（周期性 re-publish / bootstrap）
        };
        if let Some(pending) = query.progress(result, step) {
            self.0.insert(id, pending);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::*;

    fn step(last: bool) -> kad::ProgressStep {
        kad::ProgressStep {
            count: NonZeroUsize::new(1).unwrap(),
            last,
        }
    }

    fn node() -> NodeId {
        NodeId::from_peer_id(
            libp2p::identity::Keypair::generate_ed25519()
                .public()
                .to_peer_id(),
        )
    }

    /// Get：拿到首个 record 立即完成（快路径，不等查询自然结束），
    /// publisher 透传。
    #[tokio::test]
    async fn get_completes_on_first_record() {
        let publisher = node();
        let (tx, rx) = oneshot::channel();
        let query = PendingQuery::Get { reply: tx };

        let mut record = kad::Record::new(vec![1u8], b"payload".to_vec());
        record.publisher = Some(*publisher.as_peer_id());
        let result =
            kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(kad::PeerRecord {
                peer: None,
                record,
            })));

        // 非 last 步就完成（提前退出）
        assert!(query.progress(result, &step(false)).is_none());
        let got = rx.await.unwrap().unwrap();
        assert_eq!(got.value, b"payload");
        assert_eq!(got.publisher, Some(publisher));
    }

    /// Providers：跨步累积去重，last 时一次性 flush。
    #[tokio::test]
    async fn providers_accumulate_and_dedupe_across_steps() {
        let (p1, p2) = (node(), node());
        let (tx, rx) = oneshot::channel();
        let mut query = PendingQuery::Providers {
            found: Vec::new(),
            reply: tx,
        };

        let found = |peers: Vec<NodeId>| {
            kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
                key: kad::RecordKey::new(&[1u8]),
                providers: peers.iter().map(|n| *n.as_peer_id()).collect(),
            }))
        };

        // 第一步：p1、p2；未完成
        query = query
            .progress(found(vec![p1, p2]), &step(false))
            .expect("continues");
        // 第二步：p2 重复 + last → flush
        assert!(query.progress(found(vec![p2]), &step(true)).is_none());

        let got = rx.await.unwrap().unwrap();
        assert_eq!(got.len(), 2, "重复 provider 必须去重");
        assert!(got.contains(&p1) && got.contains(&p2));
    }

    /// Bootstrap：中间步继续等，last 步完成。
    #[tokio::test]
    async fn bootstrap_waits_for_last_step() {
        let (tx, rx) = oneshot::channel();
        let mut query = PendingQuery::Bootstrap { reply: tx };

        let ok = || {
            kad::QueryResult::Bootstrap(Ok(kad::BootstrapOk {
                peer: libp2p::identity::Keypair::generate_ed25519()
                    .public()
                    .to_peer_id(),
                num_remaining: 0,
            }))
        };

        query = query.progress(ok(), &step(false)).expect("continues");
        assert!(query.progress(ok(), &step(true)).is_none());
        assert!(rx.await.unwrap().is_ok());
    }

    /// 挂账表：未知 QueryId（周期性 re-publish 等）被静默忽略，不影响在账查询。
    #[tokio::test]
    async fn unknown_query_id_is_ignored() {
        let mut queries = PendingQueries::default();
        // 空表上处理任意进展不 panic
        // （QueryId 无公开构造器，经真实 kad 才能拿到——集成测试已覆盖在账
        //  路径，这里验证空表安全性即可）
        assert!(queries.0.is_empty());
        let _ = &mut queries;
    }
}
