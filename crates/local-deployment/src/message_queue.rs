//! In-memory message queue for storing messages to be sent to running or completed execution processes.

use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use ts_rs::TS;
use uuid::Uuid;

/// A queued message waiting to be sent to an execution process.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QueuedMessage {
    pub id: Uuid,
    pub task_attempt_id: Uuid,
    pub content: String,
    pub variant: Option<String>,
    pub position: usize,
    #[ts(type = "string")]
    pub created_at: DateTime<Utc>,
}

/// Request to add a message to the queue.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct AddQueuedMessageRequest {
    pub content: String,
    pub variant: Option<String>,
}

/// Request to update a queued message.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct UpdateQueuedMessageRequest {
    pub content: Option<String>,
    pub variant: Option<String>,
}

/// Request to reorder queued messages.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct ReorderQueuedMessagesRequest {
    pub message_ids: Vec<Uuid>,
}

/// In-memory store for queued messages, keyed by task_attempt_id.
#[derive(Debug, Clone)]
pub struct MessageQueueStore {
    queues: Arc<RwLock<HashMap<Uuid, Vec<QueuedMessage>>>>,
}

impl Default for MessageQueueStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageQueueStore {
    pub fn new() -> Self {
        Self {
            queues: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn list(&self, task_attempt_id: Uuid) -> Vec<QueuedMessage> {
        let queues = self.queues.read().await;
        queues.get(&task_attempt_id).cloned().unwrap_or_default()
    }

    pub async fn add(&self, task_attempt_id: Uuid, content: String, variant: Option<String>) -> QueuedMessage {
        let mut queues = self.queues.write().await;
        let queue = queues.entry(task_attempt_id).or_default();
        let position = queue.len();
        let message = QueuedMessage {
            id: Uuid::new_v4(),
            task_attempt_id,
            content,
            variant,
            position,
            created_at: Utc::now(),
        };
        queue.push(message.clone());
        message
    }

    pub async fn update(&self, task_attempt_id: Uuid, message_id: Uuid, content: Option<String>, variant: Option<String>) -> Option<QueuedMessage> {
        let mut queues = self.queues.write().await;
        let queue = queues.get_mut(&task_attempt_id)?;
        let message = queue.iter_mut().find(|m| m.id == message_id)?;
        if let Some(new_content) = content {
            message.content = new_content;
        }
        // Only update variant if explicitly provided (Some)
        // Empty string clears the variant, non-empty sets it
        if let Some(v) = variant {
            message.variant = if v.is_empty() { None } else { Some(v) };
        }
        // If variant is None, preserve the existing value
        Some(message.clone())
    }

    pub async fn remove(&self, task_attempt_id: Uuid, message_id: Uuid) -> bool {
        let mut queues = self.queues.write().await;
        let Some(queue) = queues.get_mut(&task_attempt_id) else { return false };
        let initial_len = queue.len();
        queue.retain(|m| m.id != message_id);
        if queue.len() < initial_len {
            for (idx, msg) in queue.iter_mut().enumerate() {
                msg.position = idx;
            }
            true
        } else {
            false
        }
    }

    pub async fn reorder(&self, task_attempt_id: Uuid, message_ids: Vec<Uuid>) -> Option<Vec<QueuedMessage>> {
        let mut queues = self.queues.write().await;
        let queue = queues.get_mut(&task_attempt_id)?;
        if message_ids.len() != queue.len() { return None }
        let id_set: std::collections::HashSet<Uuid> = message_ids.iter().copied().collect();
        let queue_ids: std::collections::HashSet<Uuid> = queue.iter().map(|m| m.id).collect();
        if id_set != queue_ids { return None }
        let mut msg_map: HashMap<Uuid, QueuedMessage> = queue.drain(..).map(|m| (m.id, m)).collect();
        for (idx, id) in message_ids.into_iter().enumerate() {
            if let Some(mut msg) = msg_map.remove(&id) {
                msg.position = idx;
                queue.push(msg);
            }
        }
        Some(queue.clone())
    }

    pub async fn pop_next(&self, task_attempt_id: Uuid) -> Option<QueuedMessage> {
        let mut queues = self.queues.write().await;
        let queue = queues.get_mut(&task_attempt_id)?;
        if queue.is_empty() { return None }
        let message = queue.remove(0);
        for (idx, msg) in queue.iter_mut().enumerate() {
            msg.position = idx;
        }
        Some(message)
    }

    pub async fn clear(&self, task_attempt_id: Uuid) {
        let mut queues = self.queues.write().await;
        queues.remove(&task_attempt_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_queue_add_returns_message_with_position_zero() {
        let store = MessageQueueStore::new();
        let attempt_id = Uuid::new_v4();
        let msg = store.add(attempt_id, "hello".to_string(), None).await;
        assert_eq!(msg.position, 0);
    }

    #[tokio::test]
    async fn test_queue_pop_next_returns_first_and_removes() {
        let store = MessageQueueStore::new();
        let attempt_id = Uuid::new_v4();
        store.add(attempt_id, "first".to_string(), None).await;
        store.add(attempt_id, "second".to_string(), None).await;
        let popped = store.pop_next(attempt_id).await.unwrap();
        assert_eq!(popped.content, "first");
        let remaining = store.list(attempt_id).await;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].position, 0);
    }

    #[tokio::test]
    async fn test_update_preserves_variant_when_not_provided() {
        let store = MessageQueueStore::new();
        let attempt_id = Uuid::new_v4();
        let msg = store.add(attempt_id, "content".to_string(), Some("plan".to_string())).await;

        // Update only content, not variant (variant is None)
        let updated = store.update(attempt_id, msg.id, Some("new content".to_string()), None).await.unwrap();

        // Variant should be preserved
        assert_eq!(updated.variant, Some("plan".to_string()));
        assert_eq!(updated.content, "new content");
    }

    #[tokio::test]
    async fn test_update_clears_variant_when_empty_string() {
        let store = MessageQueueStore::new();
        let attempt_id = Uuid::new_v4();
        let msg = store.add(attempt_id, "content".to_string(), Some("plan".to_string())).await;

        // Explicitly clear variant by passing empty string
        let updated = store.update(attempt_id, msg.id, None, Some("".to_string())).await.unwrap();

        // Variant should be cleared
        assert_eq!(updated.variant, None);
        assert_eq!(updated.content, "content");
    }

    #[tokio::test]
    async fn test_update_changes_variant_when_provided() {
        let store = MessageQueueStore::new();
        let attempt_id = Uuid::new_v4();
        let msg = store.add(attempt_id, "content".to_string(), Some("plan".to_string())).await;

        // Change variant to a new value
        let updated = store.update(attempt_id, msg.id, None, Some("research".to_string())).await.unwrap();

        // Variant should be updated
        assert_eq!(updated.variant, Some("research".to_string()));
    }

    #[tokio::test]
    async fn test_update_adds_variant_when_previously_none() {
        let store = MessageQueueStore::new();
        let attempt_id = Uuid::new_v4();
        let msg = store.add(attempt_id, "content".to_string(), None).await;

        // Add a variant where there was none
        let updated = store.update(attempt_id, msg.id, None, Some("plan".to_string())).await.unwrap();

        // Variant should be set
        assert_eq!(updated.variant, Some("plan".to_string()));
    }
}
