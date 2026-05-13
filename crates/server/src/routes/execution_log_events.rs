use std::time::Duration;

use axum::{
    Extension, Router,
    extract::{Query, State, ws::Message},
    response::{IntoResponse, Json as ResponseJson},
    routing::get,
};
use db::models::{
    execution_log_event::{Direction, ExecutionLogEvent, PaginatedExecutionLogEvents},
    execution_process::{ExecutionProcess, ExecutionProcessStatus},
};
use deployment::Deployment;
use serde::Deserialize;
use serde_json::json;
use utils::response::ApiResponse;

use crate::{
    DeploymentImpl,
    error::ApiError,
    middleware::signed_ws::{MaybeSignedWebSocket, SignedWsUpgrade},
};

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 500;

#[derive(Debug, Deserialize)]
pub struct EventPageQuery {
    pub after_id: Option<i64>,
    pub before_id: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct EventLiveQuery {
    pub after_id: Option<i64>,
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/events", get(get_events))
        .route("/events/latest", get(get_latest_events))
        .route("/events/live/ws", get(stream_events_live_ws))
}

fn clamp_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

async fn get_events(
    Extension(execution_process): Extension<ExecutionProcess>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<EventPageQuery>,
) -> Result<ResponseJson<ApiResponse<PaginatedExecutionLogEvents>>, ApiError> {
    let pool = &deployment.db().pool;
    let limit = clamp_limit(query.limit);
    let (cursor, direction) = if let Some(before_id) = query.before_id {
        (Some(before_id), Direction::Backward)
    } else {
        (query.after_id, Direction::Forward)
    };
    let page =
        ExecutionLogEvent::find_page(pool, execution_process.id, cursor, limit, direction).await?;

    Ok(ResponseJson(ApiResponse::success(page)))
}

async fn get_latest_events(
    Extension(execution_process): Extension<ExecutionProcess>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<EventPageQuery>,
) -> Result<ResponseJson<ApiResponse<PaginatedExecutionLogEvents>>, ApiError> {
    let pool = &deployment.db().pool;
    let page = ExecutionLogEvent::find_page(
        pool,
        execution_process.id,
        query.before_id,
        clamp_limit(query.limit),
        Direction::Backward,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(page)))
}

async fn stream_events_live_ws(
    ws: SignedWsUpgrade,
    Extension(execution_process): Extension<ExecutionProcess>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<EventLiveQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(error) =
            handle_events_live_ws(socket, deployment, execution_process, query.after_id).await
        {
            tracing::warn!("execution event live WS closed: {}", error);
        }
    })
}

async fn handle_events_live_ws(
    mut socket: MaybeSignedWebSocket,
    deployment: DeploymentImpl,
    execution_process: ExecutionProcess,
    after_id: Option<i64>,
) -> anyhow::Result<()> {
    let mut last_seen_id = after_id.unwrap_or(0);
    let pool = deployment.db().pool.clone();
    send_event_rows(&mut socket, &pool, execution_process.id, &mut last_seen_id).await?;
    socket
        .send(Message::Text(json!({"Ready": true}).to_string().into()))
        .await?;

    let mut interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let saw_terminal = send_event_rows(
                    &mut socket,
                    &pool,
                    execution_process.id,
                    &mut last_seen_id,
                ).await?;
                if saw_terminal || execution_is_settled(&pool, execution_process.id).await? {
                    socket
                        .send(Message::Text(json!({"finished": true}).to_string().into()))
                        .await?;
                    let _ = socket.close().await;
                    return Ok(());
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Ok(Some(Message::Close(_))) | Ok(None) => break,
                    Ok(Some(_)) => {}
                    Err(_) => break,
                }
            }
        }
    }

    let _ = socket.close().await;
    Ok(())
}

async fn send_event_rows(
    socket: &mut MaybeSignedWebSocket,
    pool: &sqlx::SqlitePool,
    execution_id: uuid::Uuid,
    last_seen_id: &mut i64,
) -> anyhow::Result<bool> {
    let rows = ExecutionLogEvent::find_after_id(pool, execution_id, Some(*last_seen_id), MAX_LIMIT)
        .await?;
    let mut saw_terminal = false;
    for row in rows {
        *last_seen_id = row.id;
        saw_terminal |= matches!(
            row.event_type,
            db::models::execution_log_event::ExecutionLogEventType::ExecutionFinished
        );
        socket
            .send(Message::Text(json!({"Event": row}).to_string().into()))
            .await?;
    }
    Ok(saw_terminal)
}

async fn execution_is_settled(
    pool: &sqlx::SqlitePool,
    execution_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let process = ExecutionProcess::find_by_id(pool, execution_id).await?;
    Ok(process
        .map(|process| process.status != ExecutionProcessStatus::Running)
        .unwrap_or(true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_limit_uses_safe_bounds() {
        assert_eq!(clamp_limit(None), DEFAULT_LIMIT);
        assert_eq!(clamp_limit(Some(-1)), 1);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(42)), 42);
        assert_eq!(clamp_limit(Some(10_000)), MAX_LIMIT);
    }
}
