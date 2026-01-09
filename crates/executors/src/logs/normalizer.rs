//! LogNormalizer trait and driver function for protocol-agnostic log processing.
//!
//! This module provides a generic abstraction for normalizing executor logs into
//! a common format. Each executor (ACP, Droid, Codex) can implement the `LogNormalizer`
//! trait to handle their specific log format while sharing the common driver logic.

use std::{path::Path, sync::Arc};

use json_patch::Patch;
use tokio::task::JoinHandle;
use workspace_utils::msg_store::MsgStore;

use super::utils::EntryIndexProvider;

/// Trait for normalizing executor-specific log events into conversation patches.
///
/// Each executor implements this trait with their specific event type,
/// allowing the generic `normalize_logs_with` driver function to process
/// logs from any executor using a common pattern.
pub trait LogNormalizer: Send + 'static {
    /// The executor-specific event type parsed from log lines.
    type Event: Send;

    /// Parse a log line into an event, returning None if the line is not parseable.
    fn parse_line(&self, line: &str) -> Option<Self::Event>;

    /// Extract session ID from an event, if present.
    fn extract_session_id(&self, event: &Self::Event) -> Option<String>;

    /// Process an event and return patches to apply to the conversation.
    fn process_event(
        &mut self,
        event: Self::Event,
        msg_store: &Arc<MsgStore>,
        entry_index: &EntryIndexProvider,
    ) -> Vec<Patch>;
}

/// Driver function that normalizes logs using the provided normalizer implementation.
///
/// This function spawns a background task that:
/// 1. Reads stdout lines from the MsgStore
/// 2. Parses each line using the normalizer
/// 3. Extracts session IDs and pushes them to the store
/// 4. Processes events and pushes resulting patches to the store
///
/// # Arguments
/// * `normalizer` - The executor-specific normalizer implementing LogNormalizer
/// * `msg_store` - The message store for reading logs and pushing patches
/// * `_worktree_path` - Path to the worktree (used by some normalizers for path resolution)
///
/// # Returns
/// A JoinHandle for the spawned normalization task
pub fn normalize_logs_with<N: LogNormalizer>(
    mut normalizer: N,
    msg_store: Arc<MsgStore>,
    _worktree_path: &Path,
) -> JoinHandle<()> {
    use futures::StreamExt;

    let entry_index = EntryIndexProvider::start_from(&msg_store);

    tokio::spawn(async move {
        let mut stored_session_id = false;
        let mut stdout_lines = msg_store.stdout_lines_stream();

        while let Some(Ok(line)) = stdout_lines.next().await {
            if let Some(event) = normalizer.parse_line(&line) {
                // Extract and store session ID (only once)
                if !stored_session_id
                    && let Some(session_id) = normalizer.extract_session_id(&event)
                {
                    msg_store.push_session_id(session_id);
                    stored_session_id = true;
                }

                // Process the event and push any resulting patches
                let patches = normalizer.process_event(event, &msg_store, &entry_index);
                for patch in patches {
                    msg_store.push_patch(patch);
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use workspace_utils::log_msg::LogMsg;

    use crate::logs::utils::ConversationPatch;
    use crate::logs::{NormalizedEntry, NormalizedEntryType};

    /// Mock normalizer for testing the driver function.
    ///
    /// This mock allows configurable behavior:
    /// - `session_id_to_return`: The session ID to extract from events (if any)
    /// - `patches_to_return`: Patches to return from process_event
    /// - `parse_count`: Counter for how many lines were parsed
    struct MockNormalizer {
        /// Session ID to return from extract_session_id (once, on first parseable event)
        session_id_to_return: Option<String>,
        /// Whether session ID has already been extracted
        session_id_extracted: bool,
        /// Counter for number of events processed
        process_count: Arc<AtomicUsize>,
    }

    impl MockNormalizer {
        fn new() -> Self {
            Self {
                session_id_to_return: None,
                session_id_extracted: false,
                process_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
            self.session_id_to_return = Some(session_id.into());
            self
        }

        fn process_count(&self) -> Arc<AtomicUsize> {
            self.process_count.clone()
        }
    }

    /// Mock event type - just wraps the line content
    #[derive(Debug, Clone)]
    struct MockEvent {
        content: String,
    }

    impl LogNormalizer for MockNormalizer {
        type Event = MockEvent;

        fn parse_line(&self, line: &str) -> Option<Self::Event> {
            // Parse any non-empty line as an event
            let trimmed = line.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(MockEvent {
                    content: trimmed.to_string(),
                })
            }
        }

        fn extract_session_id(&self, _event: &Self::Event) -> Option<String> {
            // Return session ID only if we have one and haven't returned it yet
            if !self.session_id_extracted {
                self.session_id_to_return.clone()
            } else {
                None
            }
        }

        fn process_event(
            &mut self,
            event: Self::Event,
            _msg_store: &Arc<MsgStore>,
            entry_index: &EntryIndexProvider,
        ) -> Vec<Patch> {
            // Mark session ID as extracted after first event
            self.session_id_extracted = true;

            // Increment process count
            self.process_count.fetch_add(1, Ordering::Relaxed);

            // Create a simple patch with a system message containing the event content
            let idx = entry_index.next();
            let entry = NormalizedEntry {
                timestamp: None,
                entry_type: NormalizedEntryType::SystemMessage,
                content: event.content,
                metadata: None,
            };

            vec![ConversationPatch::add_normalized_entry(idx, entry)]
        }
    }

    #[tokio::test]
    async fn test_normalize_logs_with_mock() {
        // Create a mock normalizer and message store
        let normalizer = MockNormalizer::new();
        let process_count = normalizer.process_count();
        let msg_store = Arc::new(MsgStore::new());

        // Spawn the normalization task
        let handle = normalize_logs_with(normalizer, msg_store.clone(), Path::new("/tmp/test"));

        // Push some test lines to stdout
        msg_store.push_stdout("line 1\n");
        msg_store.push_stdout("line 2\n");
        msg_store.push_stdout("line 3\n");

        // Signal end of stream
        msg_store.push_finished();

        // Wait for processing with timeout
        let timeout_result =
            tokio::time::timeout(Duration::from_millis(500), handle).await;
        assert!(
            timeout_result.is_ok(),
            "Normalization task should complete"
        );

        // Verify events were processed
        let final_count = process_count.load(Ordering::Relaxed);
        assert_eq!(final_count, 3, "Should have processed 3 events");

        // Verify patches were pushed to the store
        let history = msg_store.get_history();
        let patch_count = history
            .iter()
            .filter(|msg| matches!(msg, LogMsg::JsonPatch(_)))
            .count();
        assert_eq!(patch_count, 3, "Should have 3 patches in history");
    }

    #[tokio::test]
    async fn test_session_id_extraction() {
        // Create a normalizer that returns a session ID
        let normalizer = MockNormalizer::new().with_session_id("test-session-123");
        let msg_store = Arc::new(MsgStore::new());

        // Spawn the normalization task
        let handle = normalize_logs_with(normalizer, msg_store.clone(), Path::new("/tmp/test"));

        // Push test lines
        msg_store.push_stdout("first line\n");
        msg_store.push_stdout("second line\n");
        msg_store.push_finished();

        // Wait for processing
        let _ = tokio::time::timeout(Duration::from_millis(500), handle).await;

        // Verify session ID was pushed exactly once
        let history = msg_store.get_history();
        let session_ids: Vec<_> = history
            .iter()
            .filter_map(|msg| {
                if let LogMsg::SessionId(id) = msg {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(session_ids.len(), 1, "Should have exactly 1 session ID");
        assert_eq!(
            session_ids[0], "test-session-123",
            "Session ID should match"
        );
    }

    #[tokio::test]
    async fn test_patches_applied() {
        // Create a mock normalizer
        let normalizer = MockNormalizer::new();
        let msg_store = Arc::new(MsgStore::new());

        // Spawn the normalization task
        let handle = normalize_logs_with(normalizer, msg_store.clone(), Path::new("/tmp/test"));

        // Push specific content
        msg_store.push_stdout("Hello World\n");
        msg_store.push_finished();

        // Wait for processing
        let _ = tokio::time::timeout(Duration::from_millis(500), handle).await;

        // Verify the patch content
        let history = msg_store.get_history();
        let patches: Vec<_> = history
            .iter()
            .filter_map(|msg| {
                if let LogMsg::JsonPatch(patch) = msg {
                    Some(patch.clone())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(patches.len(), 1, "Should have 1 patch");

        // Verify the patch contains the expected content
        let patch_json = serde_json::to_string(&patches[0]).unwrap();
        assert!(
            patch_json.contains("Hello World"),
            "Patch should contain the event content"
        );
        assert!(
            patch_json.contains("system_message"),
            "Patch should be a system message"
        );
    }

    #[tokio::test]
    async fn test_empty_lines_ignored() {
        // Create a mock normalizer
        let normalizer = MockNormalizer::new();
        let process_count = normalizer.process_count();
        let msg_store = Arc::new(MsgStore::new());

        // Spawn the normalization task
        let handle = normalize_logs_with(normalizer, msg_store.clone(), Path::new("/tmp/test"));

        // Push lines including empty ones
        msg_store.push_stdout("line 1\n");
        msg_store.push_stdout("\n"); // empty
        msg_store.push_stdout("   \n"); // whitespace only
        msg_store.push_stdout("line 2\n");
        msg_store.push_finished();

        // Wait for processing
        let _ = tokio::time::timeout(Duration::from_millis(500), handle).await;

        // Verify only non-empty lines were processed
        let final_count = process_count.load(Ordering::Relaxed);
        assert_eq!(
            final_count, 2,
            "Should have processed only 2 non-empty events"
        );
    }

    #[tokio::test]
    async fn test_session_id_only_stored_once() {
        // Create a normalizer that would return a session ID for every event
        // But the driver should only store it once
        let normalizer = MockNormalizer::new().with_session_id("repeated-session");
        let msg_store = Arc::new(MsgStore::new());

        // Spawn the normalization task
        let handle = normalize_logs_with(normalizer, msg_store.clone(), Path::new("/tmp/test"));

        // Push multiple lines
        msg_store.push_stdout("first\n");
        msg_store.push_stdout("second\n");
        msg_store.push_stdout("third\n");
        msg_store.push_finished();

        // Wait for processing
        let _ = tokio::time::timeout(Duration::from_millis(500), handle).await;

        // Verify session ID was pushed exactly once (driver logic ensures this)
        let history = msg_store.get_history();
        let session_id_count = history
            .iter()
            .filter(|msg| matches!(msg, LogMsg::SessionId(_)))
            .count();

        assert_eq!(
            session_id_count, 1,
            "Session ID should be stored exactly once"
        );
    }
}
