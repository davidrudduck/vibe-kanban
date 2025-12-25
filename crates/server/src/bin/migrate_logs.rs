//! CLI tool to migrate JSONL logs to row-level log_entries table.
//!
//! This tool migrates execution logs from the legacy `execution_process_logs` table
//! (which stores batched JSONL) to the new `log_entries` table (which stores
//! individual rows). This migration is required for ElectricSQL compatibility.
//!
//! The migration is idempotent - running it multiple times will not create
//! duplicate entries.
//!
//! Usage:
//!   cargo run --bin migrate_logs -- --dry-run     # Dry-run (default)
//!   cargo run --bin migrate_logs -- --execute     # Actually migrate
//!   cargo run --bin migrate_logs -- --verbose     # Show details
//!   cargo run --bin migrate_logs -- --execution-id <UUID>  # Migrate single execution

use std::env;
use std::io::{self, Write};

use db::DBService;
use services::services::log_migration;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

fn print_usage() {
    println!("Migrate JSONL Logs to log_entries");
    println!();
    println!(
        "This tool migrates execution logs from execution_process_logs (JSONL batches)"
    );
    println!("to log_entries (individual rows) for ElectricSQL compatibility.");
    println!();
    println!("Usage:");
    println!(
        "  cargo run --bin migrate_logs                            Dry-run mode (default)"
    );
    println!(
        "  cargo run --bin migrate_logs -- --execute               Actually migrate logs"
    );
    println!(
        "  cargo run --bin migrate_logs -- --verbose               Show detailed progress"
    );
    println!(
        "  cargo run --bin migrate_logs -- --execution-id <UUID>   Migrate single execution"
    );
    println!(
        "  cargo run --bin migrate_logs -- --help                  Show this help"
    );
    println!();
    println!("The migration is idempotent - already migrated logs will be skipped.");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Parse arguments
    let args: Vec<String> = env::args().collect();
    let execute = args.iter().any(|a| a == "--execute");
    let verbose = args.iter().any(|a| a == "--verbose");

    // Check for execution ID argument
    let execution_id: Option<Uuid> = args
        .iter()
        .position(|a| a == "--execution-id")
        .and_then(|pos| args.get(pos + 1))
        .and_then(|id| Uuid::parse_str(id).ok());

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return Ok(());
    }

    println!("=== JSONL Log Migration Tool ===");
    println!();

    if !execute {
        println!("Running in DRY-RUN mode. No changes will be made.");
        println!("Use --execute to actually migrate logs.");
        println!();
    }

    // Connect to database
    info!("Connecting to database...");
    let db = DBService::new().await?;
    let pool = &db.pool;

    if let Some(exec_id) = execution_id {
        // Migrate single execution
        println!("Migrating logs for execution: {}", exec_id);
        println!();

        if execute {
            let result = log_migration::migrate_execution_logs(pool, exec_id).await?;

            println!("Migration complete:");
            println!("  Migrated: {}", result.migrated);
            println!("  Skipped:  {}", result.skipped);
            println!("  Errors:   {}", result.errors);
        } else {
            let result = log_migration::migrate_execution_logs_dry_run(pool, exec_id).await?;

            println!("Dry-run results:");
            println!("  Would migrate: {}", result.would_migrate);
            println!("  Would skip:    {}", result.would_skip);
            println!("  Errors:        {}", result.errors);
            println!();
            println!("Run with --execute to apply these changes.");
        }

        return Ok(());
    }

    // Get all executions with legacy logs
    info!("Searching for executions with legacy logs...");
    let execution_ids = log_migration::get_executions_with_legacy_logs(pool).await?;

    println!(
        "Found {} execution(s) with legacy logs.",
        execution_ids.len()
    );
    println!();

    if execution_ids.is_empty() {
        println!("No legacy logs to migrate. Database is up to date!");
        return Ok(());
    }

    if verbose {
        println!("Executions to process:");
        for exec_id in &execution_ids {
            println!("  - {}", exec_id);
        }
        println!();
    }

    if !execute {
        // Dry-run mode
        let mut total_would_migrate = 0;
        let mut total_would_skip = 0;
        let mut total_errors = 0;

        for exec_id in &execution_ids {
            let result = log_migration::migrate_execution_logs_dry_run(pool, *exec_id).await?;
            total_would_migrate += result.would_migrate;
            total_would_skip += result.would_skip;
            total_errors += result.errors;

            if verbose {
                println!(
                    "  {}: migrate={}, skip={}, errors={}",
                    exec_id, result.would_migrate, result.would_skip, result.errors
                );
            }
        }

        println!();
        println!("Dry-run summary:");
        println!("  Executions: {}", execution_ids.len());
        println!("  Would migrate: {}", total_would_migrate);
        println!("  Would skip: {}", total_would_skip);
        println!("  Errors: {}", total_errors);
        println!();
        println!("Run with --execute to apply these changes.");

        return Ok(());
    }

    // Execute mode - prompt for confirmation
    print!(
        "Are you sure you want to migrate logs for {} execution(s)? [y/N] ",
        execution_ids.len()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if !input.trim().eq_ignore_ascii_case("y") {
        println!("Aborted.");
        return Ok(());
    }

    println!();
    println!("Migrating logs...");

    let result = log_migration::migrate_all_logs(pool).await?;

    println!();
    println!("=== Migration Complete ===");
    println!("Executions processed: {}", result.executions_processed);
    println!("Entries migrated:     {}", result.total_migrated);
    println!("Entries skipped:      {}", result.total_skipped);
    println!("Errors:               {}", result.total_errors);

    if result.total_errors > 0 {
        warn!(
            errors = result.total_errors,
            "Some log entries could not be migrated. Check logs for details."
        );
    }

    Ok(())
}
