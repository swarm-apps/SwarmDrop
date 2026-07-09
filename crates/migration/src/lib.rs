pub use sea_orm_migration::prelude::*;

mod m20260228_000001_init;
mod m20260310_000001_save_location_enum;
mod m20260626_000001_transfer_lifecycle;
mod m20260627_000001_transfer_range_checkpoint;
mod m20260627_000002_drop_inbox;
mod m20260627_000003_trusted_device_policies;
mod m20260630_000001_inbox_fts;
mod m20260630_000002_add_transfer_origin;
mod m20260704_000001_transfer_file_local_path;
mod m20260704_000002_transfer_file_local_dir;

pub struct Migrator;

/// 测试用：迁移到 `name`（含自身）为止，之后 `down(Some(1))` 即精确回滚该迁移。
/// 各迁移的回滚测试一律用它——写死「距末尾步数」的 `up(None)+down(N)` 会被
/// 后续新增迁移破坏（已发生过两次）。
#[cfg(test)]
pub(crate) async fn up_through(db: &sea_orm::DatabaseConnection, name: &str) {
    let position = Migrator::migrations()
        .iter()
        .position(|m| m.name() == name)
        .unwrap_or_else(|| panic!("migration not found: {name}"));
    Migrator::up(db, Some(position as u32 + 1))
        .await
        .expect("run migrations");
}

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260228_000001_init::Migration),
            Box::new(m20260310_000001_save_location_enum::Migration),
            Box::new(m20260626_000001_transfer_lifecycle::Migration),
            Box::new(m20260627_000001_transfer_range_checkpoint::Migration),
            Box::new(m20260627_000002_drop_inbox::Migration),
            Box::new(m20260627_000003_trusted_device_policies::Migration),
            Box::new(m20260630_000001_inbox_fts::Migration),
            Box::new(m20260630_000002_add_transfer_origin::Migration),
            Box::new(m20260704_000001_transfer_file_local_path::Migration),
            Box::new(m20260704_000002_transfer_file_local_dir::Migration),
        ]
    }
}
