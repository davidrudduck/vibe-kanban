use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use db::models::webhook::Webhook;
use reqwest::Client;
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::sync::CancellationToken;
use utils::{log_msg::LogMsg, msg_store::MsgStore};

use futures::StreamExt;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

/// How long to cache the enabled-webhooks list before re-querying the DB.
/// Short enough that newly-added webhooks start receiving events promptly,
/// long enough to avoid a full-table scan on every broadcast event.
const WEBHOOK_CACHE_TTL: Duration = Duration::from_secs(30);

/// Subscribes to the events MsgStore and POSTs JSON-patch payloads to all
/// registered and enabled webhook URLs.
pub struct WebhookDispatcher {
    pool: SqlitePool,
    msg_store: Arc<MsgStore>,
    http: Client,
    /// Cached list of enabled webhooks plus the timestamp when the cache was
    /// last refreshed. Refreshed on a TTL so we don't hit the DB on every event.
    cache: Arc<RwLock<Option<(Instant, Vec<Webhook>)>>>,
}

impl WebhookDispatcher {
    pub fn new(pool: SqlitePool, msg_store: Arc<MsgStore>) -> Self {
        Self {
            pool,
            msg_store,
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Spawn a background task that fans out events to all enabled webhooks.
    /// The returned JoinHandle should be retained by the caller so the task is
    /// not detached. The dispatcher terminates when `shutdown` is cancelled.
    pub fn spawn(self, shutdown: CancellationToken) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run(shutdown).await;
        })
    }

    async fn enabled_webhooks(&self) -> Result<Vec<Webhook>, sqlx::Error> {
        // Fast path: cache is fresh
        {
            let cache = self.cache.read().await;
            if let Some((fetched_at, hooks)) = cache.as_ref()
                && fetched_at.elapsed() < WEBHOOK_CACHE_TTL
            {
                return Ok(hooks.clone());
            }
        }

        // Slow path: refresh cache. Another task may race us here; that's fine
        // because the query is idempotent and the last writer wins.
        let hooks = Webhook::find_enabled(&self.pool).await?;
        {
            let mut cache = self.cache.write().await;
            *cache = Some((Instant::now(), hooks.clone()));
        }
        Ok(hooks)
    }

    async fn run(self, shutdown: CancellationToken) {
        let rx = self.msg_store.get_receiver();
        let mut stream = BroadcastStream::new(rx);

        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => {
                    tracing::info!("webhook_dispatcher: shutdown signalled, exiting");
                    return;
                }
                item = stream.next() => {
                    let Some(item) = item else {
                        tracing::info!("webhook_dispatcher: event stream closed, exiting");
                        return;
                    };

                    let msg = match item {
                        Ok(m) => m,
                        Err(BroadcastStreamRecvError::Lagged(count)) => {
                            tracing::warn!(
                                "webhook_dispatcher: broadcast lagged, {} events dropped",
                                count
                            );
                            continue;
                        }
                    };

                    let payload: Option<Value> = match &msg {
                        LogMsg::JsonPatch(patch) => serde_json::to_value(patch).ok(),
                        _ => None,
                    };

                    let Some(payload) = payload else { continue };

                    let hooks = match self.enabled_webhooks().await {
                        Ok(h) => h,
                        Err(e) => {
                            tracing::warn!("webhook_dispatcher: db error loading webhooks: {e}");
                            continue;
                        }
                    };

                    if hooks.is_empty() {
                        continue;
                    }

                    // Fire-and-forget each delivery so a slow webhook can't
                    // block the dispatcher loop (head-of-line blocking).
                    for hook in hooks {
                        let client = self.http.clone();
                        let body = payload.clone();
                        tokio::spawn(async move {
                            let mut req = client
                                .post(&hook.url)
                                .header("Content-Type", "application/json")
                                .header("X-VK-Event", "patch");

                            if let Some(secret) = &hook.secret {
                                req = req.header("X-VK-Secret", secret.as_str());
                            }

                            match req.json(&body).send().await {
                                Ok(resp) if resp.status().is_success() => {}
                                Ok(resp) => {
                                    tracing::warn!(
                                        url = %hook.url,
                                        status = %resp.status(),
                                        "webhook delivery non-2xx"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(url = %hook.url, "webhook delivery error: {e}");
                                }
                            }
                        });
                    }
                }
            }
        }
    }
}
