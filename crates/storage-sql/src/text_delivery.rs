//! SQL 文本投递账本实现。
//!
//! 收件箱文本的正文和索引必须同事务提交：若任一写入失败，发送方绝不能收到 Delivered。

use entity::{
    InboxContentKind, InboxSourceKind, PeerId, TextDeliveryDirection, TextDeliveryFailure,
    TextDeliveryStatus,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use swarmdrop_host::{AppError, AppResult};
use swarmdrop_transfer::inbox::InboxItemDetail;
use swarmdrop_transfer::text_delivery::{TextDeliveryRecord, text_preview, validate_text_body};
use uuid::Uuid;

use crate::inbox;

fn record_from_model(model: entity::text_delivery::Model) -> TextDeliveryRecord {
    TextDeliveryRecord {
        delivery_id: model.delivery_id,
        direction: model.direction,
        peer_id: model.peer_id.0,
        peer_name: model.peer_name,
        body: model.body,
        status: model.status,
        failure: model.failure,
        attempt_count: model.attempt_count,
        created_at: model.created_at,
        updated_at: model.updated_at,
    }
}

fn model_from_record(record: TextDeliveryRecord) -> entity::text_delivery::ActiveModel {
    entity::text_delivery::ActiveModel {
        delivery_id: Set(record.delivery_id),
        direction: Set(record.direction),
        peer_id: Set(PeerId(record.peer_id)),
        peer_name: Set(record.peer_name),
        body: Set(record.body),
        status: Set(record.status),
        failure: Set(record.failure),
        attempt_count: Set(record.attempt_count),
        created_at: Set(record.created_at),
        updated_at: Set(record.updated_at),
    }
}

pub(crate) async fn create_outgoing(
    db: &DatabaseConnection,
    record: TextDeliveryRecord,
) -> AppResult<()> {
    if record.direction != TextDeliveryDirection::Send {
        return Err(AppError::Transfer("只能创建发送方向的文本记录".into()));
    }
    validate_text_body(&record.body)?;
    model_from_record(record).insert(db).await?;
    Ok(())
}

pub(crate) async fn get(
    db: &DatabaseConnection,
    delivery_id: Uuid,
) -> AppResult<Option<TextDeliveryRecord>> {
    Ok(entity::TextDelivery::find_by_id(delivery_id)
        .one(db)
        .await?
        .map(record_from_model))
}

pub(crate) async fn list_outgoing(
    db: &DatabaseConnection,
    peer_id: &str,
) -> AppResult<Vec<TextDeliveryRecord>> {
    Ok(entity::TextDelivery::find()
        .filter(entity::text_delivery::Column::Direction.eq(TextDeliveryDirection::Send))
        .filter(entity::text_delivery::Column::PeerId.eq(PeerId::from(peer_id)))
        .order_by_desc(entity::text_delivery::Column::UpdatedAt)
        .all(db)
        .await?
        .into_iter()
        .map(record_from_model)
        .collect())
}

pub(crate) async fn update_outgoing(
    db: &DatabaseConnection,
    delivery_id: Uuid,
    status: TextDeliveryStatus,
    failure: Option<TextDeliveryFailure>,
    attempt_count: Option<i32>,
    updated_at: i64,
) -> AppResult<()> {
    let Some(model) = entity::TextDelivery::find_by_id(delivery_id)
        .one(db)
        .await?
    else {
        return Err(AppError::Transfer("文本投递记录不存在".into()));
    };
    if model.direction != TextDeliveryDirection::Send {
        return Err(AppError::Transfer("只能更新发送方向的文本记录".into()));
    }
    let mut active = model.into_active_model();
    active.status = Set(status);
    active.failure = Set(failure);
    if let Some(attempt_count) = attempt_count {
        active.attempt_count = Set(attempt_count);
    }
    active.updated_at = Set(updated_at);
    active.update(db).await?;
    Ok(())
}

pub(crate) async fn persist_incoming(
    db: &DatabaseConnection,
    record: TextDeliveryRecord,
) -> AppResult<InboxItemDetail> {
    if record.direction != TextDeliveryDirection::Receive {
        return Err(AppError::Transfer("只能持久化接收方向的文本记录".into()));
    }
    validate_text_body(&record.body)?;

    if let Some(existing) = get(db, record.delivery_id).await? {
        return existing_incoming_detail(db, existing, &record).await;
    }

    let delivery_id = record.delivery_id;
    let body_size = i64::try_from(record.body.len())
        .map_err(|_| AppError::Transfer("文本长度超出可表示范围".into()))?;
    let received_at = record.updated_at;
    let peer_id = PeerId(record.peer_id.clone());
    let peer_name = record.peer_name.clone();
    let title = text_preview(&record.body);
    let transaction = db.begin().await?;
    model_from_record(record).insert(&transaction).await?;
    entity::inbox_item::ActiveModel {
        id: Set(Uuid::new_v4()),
        transfer_session_id: Set(None),
        text_delivery_id: Set(Some(delivery_id)),
        source_peer_id: Set(peer_id),
        source_name: Set(peer_name),
        source_kind: Set(InboxSourceKind::PairedDevice),
        content_kind: Set(InboxContentKind::Text),
        // 列表必须无需再取正文就能辨识内容；此处是正文预览，不是本地化文案。
        title: Set(title),
        item_count: Set(1),
        total_size: Set(body_size),
        root_path: Set(None),
        content_hash: Set(None),
        received_at: Set(received_at),
        last_opened_at: Set(None),
        archived_at: Set(None),
        deleted_at: Set(None),
    }
    .insert(&transaction)
    .await?;
    transaction.commit().await?;

    inbox::get_inbox_item_by_text_delivery_id(db, delivery_id)
        .await?
        .ok_or_else(|| AppError::Transfer("文本收件箱投影创建后不可读取".into()))
}

async fn existing_incoming_detail(
    db: &DatabaseConnection,
    existing: TextDeliveryRecord,
    incoming: &TextDeliveryRecord,
) -> AppResult<InboxItemDetail> {
    if existing.direction != TextDeliveryDirection::Receive
        || existing.peer_id != incoming.peer_id
        || existing.body != incoming.body
    {
        return Err(AppError::Transfer("文本投递标识与既有内容冲突".into()));
    }
    inbox::get_inbox_item_by_text_delivery_id(db, existing.delivery_id)
        .await?
        .ok_or_else(|| AppError::Transfer("既有文本投递缺少收件箱投影".into()))
}

pub(crate) async fn delete_outgoing(db: &DatabaseConnection, delivery_id: Uuid) -> AppResult<()> {
    entity::TextDelivery::delete_many()
        .filter(entity::text_delivery::Column::DeliveryId.eq(delivery_id))
        .filter(entity::text_delivery::Column::Direction.eq(TextDeliveryDirection::Send))
        .exec(db)
        .await?;
    Ok(())
}

pub(crate) async fn recover_interrupted(db: &DatabaseConnection, now_ms: i64) -> AppResult<u64> {
    let result = entity::TextDelivery::update_many()
        .col_expr(
            entity::text_delivery::Column::Status,
            sea_orm::sea_query::Expr::value(TextDeliveryStatus::Retryable),
        )
        .col_expr(
            entity::text_delivery::Column::Failure,
            sea_orm::sea_query::Expr::value(TextDeliveryFailure::TimedOut),
        )
        .col_expr(
            entity::text_delivery::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(now_ms),
        )
        .filter(entity::text_delivery::Column::Direction.eq(TextDeliveryDirection::Send))
        .filter(entity::text_delivery::Column::Status.is_in([
            TextDeliveryStatus::Sending,
            TextDeliveryStatus::WaitingConfirmation,
        ]))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ConnectOptions, Database};
    use swarmdrop_transfer::store::InboxStore;

    use super::*;
    use crate::store::SqlSessionStore;

    async fn store() -> (DatabaseConnection, SqlSessionStore) {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options
            .max_connections(1)
            .min_connections(1)
            .sqlx_logging(false);
        let db = Database::connect(options).await.expect("连接内存数据库");
        Migrator::up(&db, None).await.expect("执行迁移");
        let store = SqlSessionStore::new(Arc::new(db.clone()));
        (db, store)
    }

    fn incoming(id: Uuid, body: &str) -> TextDeliveryRecord {
        TextDeliveryRecord {
            delivery_id: id,
            direction: TextDeliveryDirection::Receive,
            peer_id: "peer-a".into(),
            peer_name: "Alice".into(),
            body: body.into(),
            status: TextDeliveryStatus::Delivered,
            failure: None,
            attempt_count: 1,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[tokio::test]
    async fn incoming_text_is_idempotent_conflict_safe_and_deleted_with_its_inbox_record() {
        let (db, store) = store().await;
        let delivery_id = Uuid::new_v4();
        let first = persist_incoming(&db, incoming(delivery_id, "first body"))
            .await
            .expect("首次写入");
        let replay = persist_incoming(&db, incoming(delivery_id, "first body"))
            .await
            .expect("同一投递重放必须收敛");
        assert_eq!(first.item.id, replay.item.id, "不得创建第二个 Inbox 条目");
        assert!(
            persist_incoming(&db, incoming(delivery_id, "tampered"))
                .await
                .is_err(),
            "同一 id 不得接收不同正文"
        );

        inbox::delete_inbox_item_record(&db, first.item.id)
            .await
            .expect("删除收件箱记录");
        assert!(
            get(&db, delivery_id).await.expect("读取账本").is_none(),
            "删除必须抹去正文"
        );
        assert!(
            store
                .get_inbox_item_detail(first.item.id)
                .await
                .expect("读取收件箱")
                .is_none()
        );
    }
}
