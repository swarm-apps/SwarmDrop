pub use sea_orm_migration::prelude::*;

mod m20260228_000001_init;
mod m20260310_000001_save_location_enum;
mod m20260626_000001_transfer_lifecycle;
mod m20260627_000001_transfer_range_checkpoint;
mod m20260627_000002_drop_inbox;
mod m20260627_000003_trusted_device_policies;
mod m20260630_000001_inbox_fts;

pub struct Migrator;

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
        ]
    }
}
