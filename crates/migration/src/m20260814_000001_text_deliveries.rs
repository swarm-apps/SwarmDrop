//! 文本投递账本与收件箱引用。
//!
//! 正文只保存在 `text_deliveries`，收件箱条目只引用稳定 delivery_id。这样发送记录不会
//! 混进 Inbox，接收端又能在一笔事务中同时写入正文与可见索引。

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260814_000001_text_deliveries"
    }
}

#[derive(DeriveIden)]
enum TextDeliveries {
    Table,
    DeliveryId,
    Direction,
    PeerId,
    PeerName,
    Body,
    Status,
    Failure,
    AttemptCount,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum InboxItems {
    Table,
    TextDeliveryId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(TextDeliveries::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TextDeliveries::DeliveryId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(TextDeliveries::Direction)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(TextDeliveries::PeerId).string().not_null())
                    .col(ColumnDef::new(TextDeliveries::PeerName).string().not_null())
                    .col(ColumnDef::new(TextDeliveries::Body).text().not_null())
                    .col(ColumnDef::new(TextDeliveries::Status).string().not_null())
                    .col(ColumnDef::new(TextDeliveries::Failure).string())
                    .col(
                        ColumnDef::new(TextDeliveries::AttemptCount)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TextDeliveries::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TextDeliveries::UpdatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // SQLite 不能在既有表上补外键；唯一索引仍把一个接收文本只能投影为一个 Inbox
        // 条目的不变量交给数据库。正文删除由同一端口事务显式编排，不依赖隐式级联。
        manager
            .alter_table(
                Table::alter()
                    .table(InboxItems::Table)
                    .add_column(ColumnDef::new(InboxItems::TextDeliveryId).uuid())
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-inbox-items-text-delivery-id")
                    .table(InboxItems::Table)
                    .col(InboxItems::TextDeliveryId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx-inbox-items-text-delivery-id")
                    .table(InboxItems::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(InboxItems::Table)
                    .drop_column(InboxItems::TextDeliveryId)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(TextDeliveries::Table).to_owned())
            .await
    }
}
