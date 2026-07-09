use sea_orm_migration::prelude::*;

/// `transfer_files.local_dir`：接收方文件最终落盘位置的**父目录 URI**,由
/// `finalize_sink` 返回后写入(桌面端为父目录绝对路径,移动端为 file:// 目录或
/// SAF 目录 document URI)。是「打开文件夹」定位真实容器目录的事实源——SAF URI
/// 无法由文件 URI 字符串推导父目录,必须由 host 侧诚实给出。
/// 历史行为 NULL——消费方(投影 content_root / 收件箱 root_path)回退到会话保存目录。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE transfer_files ADD COLUMN local_dir TEXT")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE transfer_files DROP COLUMN local_dir")
            .await?;
        Ok(())
    }
}
