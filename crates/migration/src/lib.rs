pub use sea_orm_migration::prelude::*;

mod m20260228_000001_init;
mod m20260310_000001_save_location_enum;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260228_000001_init::Migration),
            Box::new(m20260310_000001_save_location_enum::Migration),
        ]
    }
}
