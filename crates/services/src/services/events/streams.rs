use db::models::{execution_process::ExecutionProcess, scratch::Scratch, workspace::Workspace};
use futures::StreamExt;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use utils::log_msg::LogMsg;
use uuid::Uuid;

use super::{
    EventService,
    patches::execution_process_patch,
    types::{EventPatch, RecordTypes},
};

impl EventService {
    /// Stream execution processes for a specific session with initial snapshot
    /// (raw `LogMsg` format for WebSocket).
    ///
    /// Subscribes to the broadcast channel BEFORE querying the DB so that any
    /// insertion racing the snapshot is captured in the live stream rather than
    /// silently dropped.
    ///
    /// On `BroadcastStreamRecvError::Lagged`, re-emits a fresh snapshot patch
    /// rather than silently dropping it; the client's `applyUpsertPatch`
    /// treats `replace /execution_processes` as a full state reset.
    pub async fn stream_execution_processes_for_session_raw(
        &self,
        session_id: Uuid,
        show_soft_deleted: bool,
    ) -> Result<
        futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>,
        super::types::EventError,
    > {
        // Subscribe BEFORE querying the DB to avoid the race where an insertion
        // lands between the snapshot query and the live-stream subscription.
        let receiver = self.msg_store.get_receiver();

        // Build the initial snapshot.
        let processes =
            ExecutionProcess::find_by_session_id(&self.db.pool, session_id, show_soft_deleted)
                .await?;
        let processes_map: serde_json::Map<String, serde_json::Value> = processes
            .into_iter()
            .map(|p| {
                (
                    p.id.to_string(),
                    serde_json::to_value(p).expect("ExecutionProcess must be serializable to JSON"),
                )
            })
            .collect();
        let initial_patch = json!([{
            "op": "replace",
            "path": "/execution_processes",
            "value": processes_map
        }]);
        let initial_msg = LogMsg::JsonPatch(
            serde_json::from_value(initial_patch)
                .expect("hardcoded execution-processes patch structure is valid JSON Patch"),
        );

        /// Returns `None` on DB error so a transient DB hiccup during lag-recovery
        /// does not wipe the client's in-memory state to `{}`.
        async fn build_snapshot(
            pool: &sqlx::SqlitePool,
            session_id: Uuid,
            show_soft_deleted: bool,
        ) -> Option<LogMsg> {
            let processes =
                match ExecutionProcess::find_by_session_id(pool, session_id, show_soft_deleted)
                    .await
                {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(
                            session_id = %session_id,
                            error = %e,
                            "Failed to load execution processes for resync snapshot; skipping emit"
                        );
                        return None;
                    }
                };

            let processes_map: serde_json::Map<String, serde_json::Value> = processes
                .into_iter()
                .map(|p| {
                    (
                        p.id.to_string(),
                        serde_json::to_value(p)
                            .expect("ExecutionProcess must be serializable to JSON"),
                    )
                })
                .collect();

            let snapshot = json!([{
                "op": "replace",
                "path": "/execution_processes",
                "value": processes_map
            }]);
            Some(LogMsg::JsonPatch(serde_json::from_value(snapshot).expect(
                "hardcoded execution-processes patch structure is valid JSON Patch",
            )))
        }

        let live_pool = self.db.pool.clone();
        let live = BroadcastStream::new(receiver)
            .then(move |msg_result| {
                let live_pool = live_pool.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            if let Some(patch_op) = patch.0.first() {
                                if patch_op.path().starts_with("/execution_processes/") {
                                    match patch_op {
                                        json_patch::PatchOperation::Add(op) => {
                                            if let Ok(process) =
                                                serde_json::from_value::<ExecutionProcess>(
                                                    op.value.clone(),
                                                )
                                                && process.session_id == session_id
                                            {
                                                if !show_soft_deleted && process.dropped {
                                                    let remove_patch =
                                                        execution_process_patch::remove(process.id);
                                                    return Some(Ok(LogMsg::JsonPatch(
                                                        remove_patch,
                                                    )));
                                                }
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        json_patch::PatchOperation::Replace(op) => {
                                            if let Ok(process) =
                                                serde_json::from_value::<ExecutionProcess>(
                                                    op.value.clone(),
                                                )
                                                && process.session_id == session_id
                                            {
                                                if !show_soft_deleted && process.dropped {
                                                    let remove_patch =
                                                        execution_process_patch::remove(process.id);
                                                    return Some(Ok(LogMsg::JsonPatch(
                                                        remove_patch,
                                                    )));
                                                }
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        json_patch::PatchOperation::Remove(_) => {
                                            return Some(Ok(LogMsg::JsonPatch(patch)));
                                        }
                                        _ => {}
                                    }
                                }
                                // Legacy EventPatch fallback.
                                else if let Ok(event_patch_value) = serde_json::to_value(patch_op)
                                    && let Ok(event_patch) =
                                        serde_json::from_value::<EventPatch>(event_patch_value)
                                {
                                    match &event_patch.value.record {
                                        RecordTypes::ExecutionProcess(process) => {
                                            if process.session_id == session_id {
                                                if !show_soft_deleted && process.dropped {
                                                    let remove_patch =
                                                        execution_process_patch::remove(process.id);
                                                    return Some(Ok(LogMsg::JsonPatch(
                                                        remove_patch,
                                                    )));
                                                }
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        RecordTypes::DeletedExecutionProcess {
                                            session_id: Some(deleted_session_id),
                                            ..
                                        } => {
                                            if *deleted_session_id == session_id {
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)),
                        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(
                            n,
                        )) => {
                            tracing::warn!(
                                skipped = n,
                                session_id = %session_id,
                                "execution-processes stream lagged; emitting resync snapshot"
                            );
                            // Returns None on DB error — preserves client state.
                            build_snapshot(&live_pool, session_id, show_soft_deleted)
                                .await
                                .map(Ok)
                        }
                    }
                }
            })
            .filter_map(|opt| async move { opt });

        let initial_stream = futures::stream::iter(vec![Ok(initial_msg), Ok(LogMsg::Ready)]);
        Ok(initial_stream.chain(live).boxed())
    }

    /// Stream a single scratch item with initial snapshot (raw `LogMsg` format for WebSocket).
    ///
    /// On `BroadcastStreamRecvError::Lagged`, re-emits a fresh snapshot patch
    /// rather than silently dropping it; the client's `applyUpsertPatch`
    /// treats `replace /scratch` as a full state reset.
    pub async fn stream_scratch_raw(
        &self,
        scratch_id: Uuid,
        scratch_type: &db::models::scratch::ScratchType,
    ) -> Result<
        futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>,
        super::types::EventError,
    > {
        async fn build_snapshot(
            pool: &sqlx::SqlitePool,
            scratch_id: Uuid,
            scratch_type: &db::models::scratch::ScratchType,
        ) -> Option<LogMsg> {
            let scratch = match Scratch::find_by_id(pool, scratch_id, scratch_type).await {
                Ok(scratch) => scratch,
                Err(e) => {
                    tracing::warn!(
                        scratch_id = %scratch_id,
                        scratch_type = %scratch_type,
                        error = %e,
                        "Failed to load scratch for resync snapshot; skipping emit"
                    );
                    return None;
                }
            };

            let snapshot = json!([{
                "op": "replace",
                "path": "/scratch",
                "value": scratch
            }]);
            Some(LogMsg::JsonPatch(serde_json::from_value(snapshot).expect(
                "hardcoded scratch patch structure is valid JSON Patch",
            )))
        }

        // Treat errors (e.g., corrupted/malformed data) the same as "scratch not found".
        // This prevents the websocket from closing and retrying indefinitely.
        let initial_msg = build_snapshot(&self.db.pool, scratch_id, scratch_type)
            .await
            .unwrap_or_else(|| {
                let snapshot = json!([{"op": "replace", "path": "/scratch", "value": null}]);
                LogMsg::JsonPatch(
                    serde_json::from_value(snapshot)
                        .expect("fallback scratch patch is valid JSON Patch"),
                )
            });

        let id_str = scratch_id.to_string();
        let type_str = scratch_type.to_string();
        let live_pool = self.db.pool.clone();
        let live_scratch_type = scratch_type.clone();

        let live = BroadcastStream::new(self.msg_store.get_receiver())
            .then(move |msg_result| {
                let id_str = id_str.clone();
                let type_str = type_str.clone();
                let live_pool = live_pool.clone();
                let live_scratch_type = live_scratch_type.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            if let Some(op) = patch.0.first()
                                && op.path() == "/scratch"
                            {
                                let value = match op {
                                    json_patch::PatchOperation::Add(a) => Some(&a.value),
                                    json_patch::PatchOperation::Replace(r) => Some(&r.value),
                                    json_patch::PatchOperation::Remove(_) => None,
                                    _ => None,
                                };

                                let matches = value.is_some_and(|v| {
                                    let id_matches =
                                        v.get("id").and_then(|v| v.as_str()) == Some(&id_str);
                                    let type_matches = v
                                        .get("payload")
                                        .and_then(|p| p.get("type"))
                                        .and_then(|t| t.as_str())
                                        == Some(&type_str);
                                    id_matches && type_matches
                                });

                                if matches {
                                    return Some(Ok(LogMsg::JsonPatch(patch)));
                                }
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)),
                        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(
                            n,
                        )) => {
                            tracing::warn!(
                                skipped = n,
                                scratch_id = %id_str,
                                "scratch stream lagged; emitting resync snapshot"
                            );
                            // Returns None on DB error — preserves client state.
                            build_snapshot(&live_pool, scratch_id, &live_scratch_type)
                                .await
                                .map(Ok)
                        }
                    }
                }
            })
            .filter_map(|opt| async move { opt });

        let initial_stream = futures::stream::iter(vec![Ok(initial_msg), Ok(LogMsg::Ready)]);
        Ok(initial_stream.chain(live).boxed())
    }

    /// Stream all workspaces with initial snapshot (raw `LogMsg` format for WebSocket).
    ///
    /// On `BroadcastStreamRecvError::Lagged`, re-emits a fresh snapshot patch
    /// rather than silently dropping it; the client's `applyUpsertPatch`
    /// treats `replace /workspaces` as a full state reset.
    pub async fn stream_workspaces_raw(
        &self,
        archived: Option<bool>,
        limit: Option<i64>,
    ) -> Result<
        futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>,
        super::types::EventError,
    > {
        /// Returns `None` on DB error so a transient DB hiccup during lag-recovery
        /// does not wipe the client's workspace sidebar to `{}`.
        async fn build_snapshot(
            pool: &sqlx::SqlitePool,
            archived: Option<bool>,
            limit: Option<i64>,
        ) -> Option<LogMsg> {
            let workspaces = match Workspace::find_all_with_status(pool, archived, limit).await {
                Ok(ws) => ws,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Failed to load workspaces for resync snapshot; skipping emit"
                    );
                    return None;
                }
            };
            let workspaces_map: serde_json::Map<String, serde_json::Value> = workspaces
                .into_iter()
                .map(|ws| {
                    (
                        ws.id.to_string(),
                        serde_json::to_value(ws)
                            .expect("WorkspaceWithStatus must be serializable to JSON"),
                    )
                })
                .collect();

            let snapshot = json!([{
                "op": "replace",
                "path": "/workspaces",
                "value": workspaces_map
            }]);
            Some(LogMsg::JsonPatch(serde_json::from_value(snapshot).expect(
                "hardcoded workspaces patch structure is valid JSON Patch",
            )))
        }

        let initial_msg = build_snapshot(&self.db.pool, archived, limit)
            .await
            .unwrap_or_else(|| {
                // DB unavailable at subscription time — emit empty map; live patches fill it in.
                let snapshot = json!([{"op": "replace", "path": "/workspaces", "value": {}}]);
                LogMsg::JsonPatch(
                    serde_json::from_value(snapshot)
                        .expect("fallback workspaces patch is valid JSON Patch"),
                )
            });

        let live_pool = self.db.pool.clone();
        let live = BroadcastStream::new(self.msg_store.get_receiver())
            .then(move |msg_result| {
                let live_pool = live_pool.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            if let Some(op) = patch.0.first()
                                && op.path().starts_with("/workspaces")
                            {
                                if let Some(archived_filter) = archived {
                                    let value = match op {
                                        json_patch::PatchOperation::Add(a) => Some(&a.value),
                                        json_patch::PatchOperation::Replace(r) => Some(&r.value),
                                        json_patch::PatchOperation::Remove(_) => {
                                            return Some(Ok(LogMsg::JsonPatch(patch)));
                                        }
                                        _ => None,
                                    };

                                    if let Some(v) = value
                                        && let Some(ws_archived) =
                                            v.get("archived").and_then(|a| a.as_bool())
                                    {
                                        if ws_archived == archived_filter {
                                            if let json_patch::PatchOperation::Replace(r) = op {
                                                let add_patch = json_patch::Patch(vec![
                                                    json_patch::PatchOperation::Add(
                                                        json_patch::AddOperation {
                                                            path: r.path.clone(),
                                                            value: r.value.clone(),
                                                        },
                                                    ),
                                                ]);
                                                return Some(Ok(LogMsg::JsonPatch(add_patch)));
                                            }
                                            return Some(Ok(LogMsg::JsonPatch(patch)));
                                        } else {
                                            let remove_patch = json_patch::Patch(vec![
                                                json_patch::PatchOperation::Remove(
                                                    json_patch::RemoveOperation {
                                                        path: op
                                                            .path()
                                                            .to_string()
                                                            .try_into()
                                                            .expect(
                                                                "Workspace path should be valid",
                                                            ),
                                                    },
                                                ),
                                            ]);
                                            return Some(Ok(LogMsg::JsonPatch(remove_patch)));
                                        }
                                    }
                                }
                                return Some(Ok(LogMsg::JsonPatch(patch)));
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)),
                        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(
                            n,
                        )) => {
                            tracing::warn!(
                                skipped = n,
                                "workspaces stream lagged; emitting resync snapshot"
                            );
                            // Returns None on DB error — preserves client state.
                            build_snapshot(&live_pool, archived, limit).await.map(Ok)
                        }
                    }
                }
            })
            .filter_map(|opt| async move { opt });

        let initial_stream = futures::stream::iter(vec![Ok(initial_msg), Ok(LogMsg::Ready)]);
        Ok(initial_stream.chain(live).boxed())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use db::models::{
        scratch::{CreateScratch, Scratch, ScratchPayload, ScratchType},
        workspace::{CreateWorkspace, Workspace},
    };
    use futures::StreamExt;
    use json_patch::PatchOperation;
    use sqlx::SqlitePool;
    use tokio::{
        sync::RwLock,
        time::{Duration, timeout},
    };
    use utils::{log_msg::LogMsg, msg_store::MsgStore};
    use uuid::Uuid;

    use super::EventService;

    /// Returns both the service and the underlying pool so tests can seed DB rows.
    async fn make_event_service() -> (EventService, SqlitePool) {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        sqlx::migrate!("../db/migrations")
            .run(&pool)
            .await
            .expect("migrations");
        let db = db::DBService { pool: pool.clone() };
        // Tiny capacity (2) so pushing 16 messages forces BroadcastStreamRecvError::Lagged.
        let msg_store = Arc::new(MsgStore::with_capacity(2));
        let entry_count = Arc::new(RwLock::new(0usize));
        (EventService::new(db, msg_store, entry_count), pool)
    }

    /// Extract the value field of a `replace <root>` patch, panicking on mismatch.
    fn extract_replace_value<'a>(msg: &'a LogMsg, root: &str) -> &'a serde_json::Value {
        match msg {
            LogMsg::JsonPatch(p) => match p.0.first() {
                Some(PatchOperation::Replace(r)) if r.path.as_str() == root => &r.value,
                other => panic!("expected replace {root}, got first op: {other:?}"),
            },
            _ => panic!("expected JsonPatch, got {msg:?}"),
        }
    }

    fn first_op_path(msg: &LogMsg) -> Option<String> {
        match msg {
            LogMsg::JsonPatch(p) => p.0.first().map(|op| op.path().to_string()),
            _ => None,
        }
    }

    #[tokio::test]
    async fn workspaces_stream_resyncs_after_lag() {
        let (svc, pool) = make_event_service().await;

        // Seed workspace_1 BEFORE opening the stream — must appear in the initial snapshot.
        let ws_id_1 = Uuid::new_v4();
        Workspace::create(
            &pool,
            &CreateWorkspace {
                branch: "branch-1".into(),
                name: Some("WS1".into()),
            },
            ws_id_1,
        )
        .await
        .expect("create workspace 1");

        let mut stream = svc
            .stream_workspaces_raw(Some(false), None)
            .await
            .expect("stream");

        // Initial snapshot — must contain ws_id_1.
        let initial = stream.next().await.unwrap().unwrap();
        let init_value = extract_replace_value(&initial, "/workspaces");
        assert!(
            init_value.get(ws_id_1.to_string().as_str()).is_some(),
            "initial snapshot must include ws_id_1; got value={init_value}"
        );

        let ready = stream.next().await.unwrap().unwrap();
        assert!(
            matches!(ready, LogMsg::Ready),
            "expected Ready, got {ready:?}"
        );

        // Flood the broadcast past capacity (2) to force Lagged on next poll.
        for _ in 0..16 {
            svc.msg_store().push(LogMsg::Stdout("noise".into()));
        }

        // Seed workspace_2 AFTER flooding — proves the resync re-queries the DB.
        let ws_id_2 = Uuid::new_v4();
        Workspace::create(
            &pool,
            &CreateWorkspace {
                branch: "branch-2".into(),
                name: Some("WS2".into()),
            },
            ws_id_2,
        )
        .await
        .expect("create workspace 2");

        // After lag, must receive a fresh resync snapshot containing BOTH workspaces.
        let next = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timed out — stream yielded nothing after lag")
            .unwrap()
            .unwrap();
        let resync_value = extract_replace_value(&next, "/workspaces");
        assert!(
            resync_value.get(ws_id_1.to_string().as_str()).is_some(),
            "resync must include ws_id_1; got value={resync_value}"
        );
        assert!(
            resync_value.get(ws_id_2.to_string().as_str()).is_some(),
            "resync must include ws_id_2 (inserted post-flood — proves DB was re-queried); \
             got value={resync_value}"
        );
    }

    #[tokio::test]
    async fn scratch_stream_resyncs_after_lag() {
        let (svc, pool) = make_event_service().await;
        let scratch_id = Uuid::new_v4();
        let scratch_type = ScratchType::DraftTask;

        // Seed the scratch BEFORE opening the stream.
        Scratch::create(
            &pool,
            scratch_id,
            &CreateScratch {
                payload: ScratchPayload::DraftTask("initial content".into()),
            },
        )
        .await
        .expect("create scratch");

        let mut stream = svc
            .stream_scratch_raw(scratch_id, &scratch_type)
            .await
            .expect("stream");

        // Initial snapshot — must contain the seeded scratch (not null).
        let initial = stream.next().await.unwrap().unwrap();
        let init_value = extract_replace_value(&initial, "/scratch");
        assert!(
            !init_value.is_null(),
            "initial snapshot value must be non-null; got {init_value}"
        );
        assert_eq!(
            init_value.get("id").and_then(|v| v.as_str()),
            Some(scratch_id.to_string().as_str()),
            "initial snapshot must have the correct scratch id"
        );

        let ready = stream.next().await.unwrap().unwrap();
        assert!(
            matches!(ready, LogMsg::Ready),
            "expected Ready, got {ready:?}"
        );

        // Flood the broadcast past capacity (2) to force Lagged on next poll.
        for _ in 0..16 {
            svc.msg_store().push(LogMsg::Stdout("noise".into()));
        }

        // After lag, must receive a fresh resync snapshot with the correct scratch.
        let next = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timed out — stream yielded nothing after lag")
            .unwrap()
            .unwrap();
        let resync_value = extract_replace_value(&next, "/scratch");
        assert!(
            !resync_value.is_null(),
            "resync snapshot value must be non-null; got {resync_value}"
        );
        assert_eq!(
            resync_value.get("id").and_then(|v| v.as_str()),
            Some(scratch_id.to_string().as_str()),
            "resync snapshot must contain the correct scratch id"
        );
    }

    #[tokio::test]
    async fn execution_processes_stream_resyncs_after_lag() {
        let (svc, _pool) = make_event_service().await;
        let session_id = Uuid::new_v4();

        let mut stream = svc
            .stream_execution_processes_for_session_raw(session_id, false)
            .await
            .expect("stream");

        // Initial snapshot — empty DB → value must be an empty JSON object `{}`.
        let initial = stream.next().await.unwrap().unwrap();
        let init_value = extract_replace_value(&initial, "/execution_processes");
        assert!(
            init_value.is_object(),
            "initial snapshot value must be a JSON object; got {init_value}"
        );
        assert!(
            init_value.as_object().unwrap().is_empty(),
            "initial snapshot must be empty for an unseeded session; got {init_value}"
        );

        let ready = stream.next().await.unwrap().unwrap();
        assert!(
            matches!(ready, LogMsg::Ready),
            "expected Ready, got {ready:?}"
        );

        // Flood the broadcast past capacity (2) to force Lagged on next poll.
        for _ in 0..16 {
            svc.msg_store().push(LogMsg::Stdout("noise".into()));
        }

        // After lag, must receive a fresh resync snapshot (same empty map — DB unchanged).
        let next = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timed out — stream yielded nothing after lag")
            .unwrap()
            .unwrap();
        let resync_value = extract_replace_value(&next, "/execution_processes");
        assert!(
            resync_value.is_object(),
            "resync snapshot value must be a JSON object; got path={:?}",
            first_op_path(&next)
        );
        assert!(
            resync_value.as_object().unwrap().is_empty(),
            "resync snapshot must be empty for an unseeded session; got {resync_value}"
        );
    }
}
