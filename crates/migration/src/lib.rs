pub use sea_orm_migration::prelude::*;

mod m20260228_000001_init;
mod m20260310_000001_save_location_enum;
mod m20260626_000001_transfer_lifecycle;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260228_000001_init::Migration),
            Box::new(m20260310_000001_save_location_enum::Migration),
            Box::new(m20260626_000001_transfer_lifecycle::Migration),
        ]
    }
}
