use services::services::execution_log_event_migration::migrate_legacy_execution_logs_to_events;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = db::DBService::new_migration_pool().await?;
    let summary = migrate_legacy_execution_logs_to_events(&pool).await?;
    eprintln!(
        "execution-log-event-migration: processed={} migrated={} skipped_invalid_lines={}",
        summary.processed_executions, summary.migrated_events, summary.skipped_invalid_lines
    );
    pool.close().await;
    Ok(())
}
