use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 将旧格式 save_path 裸字符串迁移为 JSON 格式的 SaveLocation
        //
        // Android 端旧数据：save_path = "Download"
        //   → {"type":"androidPublicDir","subdir":"SwarmDrop"}
        //
        // 桌面端旧数据：save_path = "/home/user/Downloads/SwarmDrop" 或 "C:\..."
        //   → {"type":"path","path":"..."}
        //
        // NULL 值保持不变

        // 1. 先处理 Android 端固定值 "Download"
        db.execute_unprepared(
            r#"UPDATE transfer_sessions
               SET save_path = '{"type":"androidPublicDir","subdir":"SwarmDrop"}'
               WHERE save_path = 'Download'"#,
        )
        .await?;

        // 2. 处理桌面端路径（非 NULL、非 JSON 格式的字符串）
        //    检测方式：不以 '{' 开头的非 NULL 值（排除已迁移的数据）
        db.execute_unprepared(
            r#"UPDATE transfer_sessions
               SET save_path = '{"type":"path","path":' || json_quote(save_path) || '}'
               WHERE save_path IS NOT NULL
                 AND save_path NOT LIKE '{%'"#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 回滚：将 JSON 格式还原为裸字符串
        // Android 端
        db.execute_unprepared(
            r#"UPDATE transfer_sessions
               SET save_path = 'Download'
               WHERE save_path LIKE '%"androidPublicDir"%'"#,
        )
        .await?;

        // 桌面端：提取 path 字段值
        db.execute_unprepared(
            r#"UPDATE transfer_sessions
               SET save_path = json_extract(save_path, '$.path')
               WHERE save_path LIKE '%"path"%'
                 AND json_extract(save_path, '$.type') = 'path'"#,
        )
        .await?;

        Ok(())
    }
}
