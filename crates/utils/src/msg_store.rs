use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

use futures::{StreamExt, future};
use tokio::{sync::broadcast, task::JoinHandle};
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};

use crate::{log_msg::LogMsg, stream_lines::LinesStreamExt};

// 100 MB Limit
const HISTORY_BYTES: usize = 100000 * 1024;

#[derive(Clone)]
struct StoredMsg {
    msg: LogMsg,
    bytes: usize,
}

struct Inner {
    history: VecDeque<StoredMsg>,
    total_bytes: usize,
}

pub struct MsgStore {
    inner: RwLock<Inner>,
    sender: broadcast::Sender<LogMsg>,
}

impl Default for MsgStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MsgStore {
    pub fn new() -> Self {
        Self::with_capacity(100000)
    }

    /// Create a `MsgStore` with a custom broadcast channel capacity. Useful in
    /// tests where a small capacity triggers lag/resync paths.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            inner: RwLock::new(Inner {
                history: VecDeque::with_capacity(32),
                total_bytes: 0,
            }),
            sender,
        }
    }

    pub fn push(&self, msg: LogMsg) {
        let _ = self.sender.send(msg.clone()); // live listeners
        let bytes = msg.approx_bytes();

        let mut inner = self.inner.write().unwrap();
        while inner.total_bytes.saturating_add(bytes) > HISTORY_BYTES {
            if let Some(front) = inner.history.pop_front() {
                inner.total_bytes = inner.total_bytes.saturating_sub(front.bytes);
            } else {
                break;
            }
        }
        inner.history.push_back(StoredMsg { msg, bytes });
        inner.total_bytes = inner.total_bytes.saturating_add(bytes);
    }

    // Convenience
    pub fn push_stdout<S: Into<String>>(&self, s: S) {
        self.push(LogMsg::Stdout(s.into()));
    }

    pub fn push_patch(&self, patch: json_patch::Patch) {
        self.push(LogMsg::JsonPatch(patch));
    }

    pub fn push_session_id(&self, session_id: String) {
        self.push(LogMsg::SessionId(session_id));
    }

    pub fn push_message_id(&self, id: String) {
        self.push(LogMsg::MessageId(id));
    }

    pub fn push_finished(&self) {
        self.push(LogMsg::Finished);
    }

    /// Subscribe to the live broadcast stream.
    ///
    /// **Invariant**: if you also call [`get_history`][Self::get_history], always
    /// call `get_receiver()` **first**.  [`push`][Self::push] broadcasts before
    /// acquiring the history write-lock, so a push that races with a snapshot read
    /// will be captured by a pre-existing receiver even when it misses the history
    /// snapshot.  Subscribing after reading history creates a gap where such a push
    /// is silently lost.
    pub fn get_receiver(&self) -> broadcast::Receiver<LogMsg> {
        self.sender.subscribe()
    }

    pub fn get_history(&self) -> Vec<LogMsg> {
        self.inner
            .read()
            .unwrap()
            .history
            .iter()
            .map(|s| s.msg.clone())
            .collect()
    }

    /// History then live, as `LogMsg`.
    pub fn history_plus_stream(
        &self,
    ) -> futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>> {
        // Subscribe first so any push() that races with get_history() is
        // captured in the live stream. push() broadcasts before writing to
        // history, so subscribe-then-read is the correct ordering.
        let rx = self.get_receiver();
        let history = self.get_history();

        let hist = futures::stream::iter(history.into_iter().map(Ok::<_, std::io::Error>));
        let live = BroadcastStream::new(rx).filter_map(|res| async move {
            match res {
                Ok(msg) => Some(Ok(msg)),
                Err(BroadcastStreamRecvError::Lagged(n)) => {
                    tracing::error!(
                        skipped = n,
                        "MsgStore broadcast lagged. {n} messages dropped for this subscriber"
                    );
                    None
                }
            }
        });

        Box::pin(hist.chain(live))
    }

    pub fn stdout_chunked_stream(
        &self,
    ) -> futures::stream::BoxStream<'static, Result<String, std::io::Error>> {
        self.history_plus_stream()
            .take_while(|res| future::ready(!matches!(res, Ok(LogMsg::Finished))))
            .filter_map(|res| async move {
                match res {
                    Ok(LogMsg::Stdout(s)) => Some(Ok(s)),
                    _ => None,
                }
            })
            .boxed()
    }

    pub fn stdout_lines_stream(
        &self,
    ) -> futures::stream::BoxStream<'static, std::io::Result<String>> {
        self.stdout_chunked_stream().lines()
    }

    pub fn stderr_chunked_stream(
        &self,
    ) -> futures::stream::BoxStream<'static, Result<String, std::io::Error>> {
        self.history_plus_stream()
            .take_while(|res| future::ready(!matches!(res, Ok(LogMsg::Finished))))
            .filter_map(|res| async move {
                match res {
                    Ok(LogMsg::Stderr(s)) => Some(Ok(s)),
                    _ => None,
                }
            })
            .boxed()
    }

    /// Forward a stream of typed log messages into this store.
    pub fn spawn_forwarder<S, E>(self: Arc<Self>, stream: S) -> JoinHandle<()>
    where
        S: futures::Stream<Item = Result<LogMsg, E>> + Send + 'static,
        E: std::fmt::Display + Send + 'static,
    {
        tokio::spawn(async move {
            tokio::pin!(stream);

            while let Some(next) = stream.next().await {
                match next {
                    Ok(msg) => self.push(msg),
                    Err(e) => self.push(LogMsg::Stderr(format!("stream error: {e}"))),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use tokio::time::{Duration, timeout};
    use tokio_stream::wrappers::BroadcastStream;

    use super::*;

    /// Basic contract: messages pushed before stream creation appear in history replay;
    /// messages pushed after appear in the live segment.
    #[tokio::test]
    async fn history_plus_stream_replays_history_then_live() {
        let store = MsgStore::new();

        store.push(LogMsg::Stdout("before".into()));

        let mut stream = store.history_plus_stream();

        store.push(LogMsg::Stdout("after".into()));

        let msg1 = timeout(Duration::from_millis(200), stream.next())
            .await
            .expect("timed out waiting for first message")
            .expect("stream ended early");
        let msg2 = timeout(Duration::from_millis(200), stream.next())
            .await
            .expect("timed out waiting for second message")
            .expect("stream ended early");

        assert!(
            matches!(msg1, Ok(LogMsg::Stdout(ref s)) if s == "before"),
            "first message should be history replay: got {msg1:?}"
        );
        assert!(
            matches!(msg2, Ok(LogMsg::Stdout(ref s)) if s == "after"),
            "second message should be live: got {msg2:?}"
        );
    }

    /// Race-safety: a push() that arrives AFTER get_history() but for which we
    /// were already subscribed must appear in the combined stream.
    ///
    /// The fix in `history_plus_stream()` calls `get_receiver()` **before**
    /// `get_history()`.  This test simulates the race-window scenario:
    ///
    /// ```text
    ///   get_receiver()   ← subscribed; any subsequent push() lands in rx
    ///   get_history()    ← "race" not yet pushed; absent from snapshot
    ///   push("race")     ← fires AFTER history read; only path is live broadcast
    /// ```
    ///
    /// If the ordering were reversed (`get_history()` first, then `get_receiver()`),
    /// the push in step 3 would arrive before the subscription exists and be lost.
    /// No concurrency is needed to reproduce this: the three steps are executed
    /// sequentially and the invariant is deterministic.
    #[tokio::test]
    async fn history_plus_stream_subscribe_first_captures_race_window_push() {
        let store = MsgStore::new();

        // Baseline message — already in history before we start.
        store.push(LogMsg::Stdout("A".into()));

        // Step 1: subscribe FIRST (mirrors the fix in history_plus_stream).
        let rx = store.get_receiver();

        // Step 2: read history — "race" has not been pushed yet, so it is absent.
        let history = store.get_history();

        // Guard: confirm "race" is genuinely absent from the snapshot.  If this
        // assertion fires the test premise is broken and the final assertion below
        // would pass vacuously (via history replay rather than the live segment).
        assert!(
            !history
                .iter()
                .any(|m| matches!(m, LogMsg::Stdout(s) if s == "race")),
            "\"race\" must not be in the history snapshot before it is pushed"
        );

        // Step 3: push "race" AFTER get_history().  Because rx was subscribed
        // before get_history(), this push is captured by the live segment even
        // though it is absent from the history snapshot.
        store.push(LogMsg::Stdout("race".into()));

        // Build the same combined stream that history_plus_stream() builds.
        let hist = futures::stream::iter(history.into_iter().map(Ok::<_, std::io::Error>));
        let live = BroadcastStream::new(rx).filter_map(|res| async move {
            match res {
                Ok(msg) => Some(Ok(msg)),
                Err(_) => None,
            }
        });
        let combined = hist.chain(live);
        tokio::pin!(combined);

        // Collect up to 5 messages with a short timeout each.
        let mut msgs: Vec<String> = Vec::new();
        for _ in 0..5 {
            match timeout(Duration::from_millis(100), combined.next()).await {
                Ok(Some(Ok(LogMsg::Stdout(s)))) => msgs.push(s),
                _ => break,
            }
        }

        assert!(
            msgs.contains(&"A".to_string()),
            "msg_A must appear via history replay; got {msgs:?}"
        );
        assert!(
            msgs.contains(&"race".to_string()),
            "race-window push must be captured by the live stream segment \
             because rx was subscribed before get_history(); got {msgs:?}"
        );
    }
}
