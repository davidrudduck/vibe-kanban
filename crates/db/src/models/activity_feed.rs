//! Activity Feed model for notification popover.
//!
//! Provides categorized activity items for the bell icon notification UI.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;
use uuid::Uuid;

use super::task::TaskStatus;

/// Category for activity feed items.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ActivityCategory {
    NeedsReview,
    InProgress,
    Completed,
}

/// A single activity feed item.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityFeedItem {
    pub task_id: Uuid,
    pub task_title: String,
    pub project_id: Uuid,
    pub project_name: String,
    pub status: TaskStatus,
    pub category: ActivityCategory,
    pub executor: String,
    pub activity_at: DateTime<Utc>,
    /// Whether this item has been dismissed by the user.
    pub is_dismissed: bool,
}

/// Counts per category for badge display.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityCounts {
    pub needs_review: usize,
    pub in_progress: usize,
    pub completed: usize,
    /// Number of dismissed items (across all categories).
    pub dismissed: usize,
}

/// Activity feed response with items and counts.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityFeed {
    pub items: Vec<ActivityFeedItem>,
    pub counts: ActivityCounts,
}

impl ActivityFeed {
    /// Fetch activity feed with categorized items.
    ///
    /// Categories:
    /// - `needs_review`: Tasks with status = 'inreview'
    /// - `in_progress`: Tasks with status = 'inprogress' AND has running execution process
    /// - `completed`: Tasks with status = 'done' AND updated within last 24 hours
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `include_dismissed` - If true, includes dismissed items; if false, excludes them
    pub async fn fetch(pool: &SqlitePool, include_dismissed: bool) -> Result<Self, sqlx::Error> {
        let cutoff_24h = Utc::now() - Duration::hours(24);

        let records = sqlx::query!(
            r#"SELECT
  t.id                            AS "task_id!: Uuid",
  t.title                         AS task_title,
  t.project_id                    AS "project_id!: Uuid",
  p.name                          AS project_name,
  t.status                        AS "status!: TaskStatus",
  COALESCE(t.activity_at, t.created_at) AS "activity_at!: DateTime<Utc>",

  COALESCE((
    SELECT ta.executor
      FROM task_attempts ta
      WHERE ta.task_id = t.id
     ORDER BY ta.created_at DESC
      LIMIT 1
  ), '')                          AS "executor!: String",

  CASE WHEN EXISTS (
    SELECT 1
      FROM task_attempts ta
      JOIN execution_processes ep
        ON ep.task_attempt_id = ta.id
     WHERE ta.task_id       = t.id
       AND ep.status        = 'running'
       AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     LIMIT 1
  ) THEN 1 ELSE 0 END            AS "has_running_attempt!: i64",

  CASE WHEN EXISTS (
    SELECT 1 FROM activity_dismissals ad WHERE ad.task_id = t.id
  ) THEN 1 ELSE 0 END            AS "is_dismissed!: i64"

FROM tasks t
JOIN projects p ON t.project_id = p.id
WHERE
  (
    -- Needs Review: InReview status
    t.status = 'inreview'
    OR
    -- In Progress: InProgress status AND has running execution process
    (t.status = 'inprogress' AND EXISTS (
      SELECT 1
        FROM task_attempts ta
        JOIN execution_processes ep
          ON ep.task_attempt_id = ta.id
       WHERE ta.task_id = t.id
         AND ep.status = 'running'
         AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
    ))
    OR
    -- Completed: Done status within last 24h
    (t.status = 'done' AND t.activity_at > $1)
  )

ORDER BY t.activity_at DESC"#,
            cutoff_24h
        )
        .fetch_all(pool)
        .await?;

        let mut items = Vec::new();
        let mut needs_review_count = 0;
        let mut in_progress_count = 0;
        let mut completed_count = 0;
        let mut dismissed_count = 0;

        for rec in records {
            let is_dismissed = rec.is_dismissed != 0;

            // Track dismissed count for all matching items
            if is_dismissed {
                dismissed_count += 1;
            }

            // Skip dismissed items if not including them
            if is_dismissed && !include_dismissed {
                continue;
            }

            let category = if rec.status == TaskStatus::InReview {
                if !is_dismissed {
                    needs_review_count += 1;
                }
                ActivityCategory::NeedsReview
            } else if rec.status == TaskStatus::InProgress && rec.has_running_attempt != 0 {
                if !is_dismissed {
                    in_progress_count += 1;
                }
                ActivityCategory::InProgress
            } else if rec.status == TaskStatus::Done {
                if !is_dismissed {
                    completed_count += 1;
                }
                ActivityCategory::Completed
            } else {
                // Skip items that don't match a category (shouldn't happen given WHERE clause)
                continue;
            };

            items.push(ActivityFeedItem {
                task_id: rec.task_id,
                task_title: rec.task_title,
                project_id: rec.project_id,
                project_name: rec.project_name,
                status: rec.status,
                category,
                executor: rec.executor,
                activity_at: rec.activity_at,
                is_dismissed,
            });
        }

        Ok(ActivityFeed {
            items,
            counts: ActivityCounts {
                needs_review: needs_review_count,
                in_progress: in_progress_count,
                completed: completed_count,
                dismissed: dismissed_count,
            },
        })
    }
}
