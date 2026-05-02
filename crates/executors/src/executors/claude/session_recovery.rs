use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use ts_rs::TS;

const MAX_SESSIONS_TO_SCAN: usize = 50;
const TAIL_BUFFER_SIZE: u64 = 16384;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct AvailableSessionInfo {
    pub session_id: String,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_secs: Option<i64>,
    pub file_size: u64,
}

fn is_valid_session_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn encode_worktree_path(worktree_path: &Path) -> String {
    worktree_path.to_string_lossy().replace('/', "-")
}

fn claude_projects_dir(worktree_path: &Path) -> Option<PathBuf> {
    let encoded = encode_worktree_path(worktree_path);
    let home = dirs::home_dir()?;
    Some(home.join(".claude").join("projects").join(encoded))
}

pub async fn scan_available_sessions(worktree_path: &Path) -> Vec<AvailableSessionInfo> {
    let dir = match claude_projects_dir(worktree_path) {
        Some(d) => d,
        None => {
            tracing::warn!("Could not determine home directory; skipping session scan");
            return Vec::new();
        }
    };

    let mut entries = match tokio::fs::read_dir(&dir).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::debug!(
                "Could not read claude projects dir {}: {}",
                dir.display(),
                e
            );
            return Vec::new();
        }
    };

    let mut sessions = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        if sessions.len() >= MAX_SESSIONS_TO_SCAN {
            tracing::debug!("Session scan capped at {} entries", MAX_SESSIONS_TO_SCAN);
            break;
        }

        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }

        let session_id = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) if is_valid_session_id(s) => s.to_string(),
            _ => continue,
        };

        let file_size = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
        if file_size == 0 {
            continue;
        }

        let (start_time, end_time) = extract_timestamps(&path, file_size).await;
        let duration_secs = match (start_time, end_time) {
            (Some(s), Some(e)) => {
                let secs = (e - s).num_seconds();
                if secs >= 0 { Some(secs) } else { None }
            }
            _ => None,
        };

        sessions.push(AvailableSessionInfo {
            session_id,
            start_time,
            end_time,
            duration_secs,
            file_size,
        });
    }

    sessions.sort_by(|a, b| b.end_time.cmp(&a.end_time));
    sessions
}

fn parse_first_timestamp(data: &str) -> Option<DateTime<Utc>> {
    for line in data.lines() {
        if let Some(ts) = extract_timestamp_from_line(line) {
            return Some(ts);
        }
    }
    None
}

fn parse_last_timestamp(data: &str) -> Option<DateTime<Utc>> {
    let mut last = None;
    for line in data.lines() {
        if let Some(ts) = extract_timestamp_from_line(line) {
            last = Some(ts);
        }
    }
    last
}

fn extract_timestamp_from_line(line: &str) -> Option<DateTime<Utc>> {
    let obj: serde_json::Value = serde_json::from_str(line).ok()?;
    let ts_str = obj.get("timestamp")?.as_str()?;
    ts_str.parse::<DateTime<Utc>>().ok()
}

async fn extract_timestamps(
    path: &Path,
    file_size: u64,
) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let mut file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return (None, None),
    };

    // For small files, read the whole thing
    if file_size <= TAIL_BUFFER_SIZE * 2 {
        let mut content = String::new();
        if file.read_to_string(&mut content).await.is_err() {
            return (None, None);
        }
        return (
            parse_first_timestamp(&content),
            parse_last_timestamp(&content),
        );
    }

    // Read first chunk for start_time
    let mut head_buf = vec![0u8; TAIL_BUFFER_SIZE as usize];
    let head_bytes = match file.read(&mut head_buf).await {
        Ok(n) => n,
        Err(_) => return (None, None),
    };
    let head_str = String::from_utf8_lossy(&head_buf[..head_bytes]);
    let first_ts = parse_first_timestamp(&head_str);

    // Seek to end minus buffer for end_time
    let seek_pos = file_size.saturating_sub(TAIL_BUFFER_SIZE);
    if file.seek(std::io::SeekFrom::Start(seek_pos)).await.is_err() {
        return (first_ts, None);
    }
    let mut tail_buf = vec![0u8; TAIL_BUFFER_SIZE as usize];
    let tail_bytes = match file.read(&mut tail_buf).await {
        Ok(n) => n,
        Err(_) => return (first_ts, None),
    };
    let tail_str = String::from_utf8_lossy(&tail_buf[..tail_bytes]);
    let last_ts = parse_last_timestamp(&tail_str);

    (first_ts, last_ts)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn test_encode_worktree_path() {
        assert_eq!(
            encode_worktree_path(Path::new(
                "/var/tmp/vibe-kanban-dev/worktrees/623f-saturday-context"
            )),
            "-var-tmp-vibe-kanban-dev-worktrees-623f-saturday-context"
        );
    }

    #[test]
    fn test_encode_root_path() {
        assert_eq!(
            encode_worktree_path(Path::new("/data/Code/vibe-kanban")),
            "-data-Code-vibe-kanban"
        );
    }

    #[test]
    fn test_valid_session_ids() {
        assert!(is_valid_session_id("abc123"));
        assert!(is_valid_session_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(is_valid_session_id("a"));
    }

    #[test]
    fn test_invalid_session_ids() {
        assert!(!is_valid_session_id(""));
        assert!(!is_valid_session_id("../../etc/passwd"));
        assert!(!is_valid_session_id("session id with spaces"));
        assert!(!is_valid_session_id("session;rm -rf /"));
        assert!(!is_valid_session_id(&"a".repeat(200)));
    }

    #[test]
    fn test_extract_timestamp_from_line() {
        let line = r#"{"type":"message","timestamp":"2024-01-15T10:30:00Z","content":"hello"}"#;
        let ts = extract_timestamp_from_line(line);
        assert!(ts.is_some());
        assert_eq!(ts.unwrap().to_rfc3339(), "2024-01-15T10:30:00+00:00");
    }

    #[test]
    fn test_extract_timestamp_invalid_json() {
        assert!(extract_timestamp_from_line("not json").is_none());
        assert!(extract_timestamp_from_line("").is_none());
        assert!(extract_timestamp_from_line(r#"{"no_timestamp": true}"#).is_none());
    }

    #[test]
    fn test_claude_projects_dir_returns_none_without_home() {
        // This tests the Option return — home_dir() is system-dependent
        // so we just verify the function signature works with the Option
        let result = claude_projects_dir(Path::new("/some/path"));
        // On CI/test systems home_dir() should return Some
        if let Some(dir) = result {
            assert!(dir.to_string_lossy().contains(".claude"));
            assert!(dir.to_string_lossy().contains("projects"));
        }
    }
}
