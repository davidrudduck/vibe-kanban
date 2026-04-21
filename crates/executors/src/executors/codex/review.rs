use std::sync::Arc;

use codex_app_server_protocol::{ReviewTarget, ThreadStartParams};

use super::{SandboxMode, client::AppServerClient};
use crate::executors::ExecutorError;

pub async fn launch_codex_review(
    thread_start_params: ThreadStartParams,
    resume_session: Option<String>,
    sandbox: Option<SandboxMode>,
    review_target: ReviewTarget,
    client: Arc<AppServerClient>,
) -> Result<(), ExecutorError> {
    let account = client.get_account().await?;
    if account.requires_openai_auth && account.account.is_none() {
        return Err(ExecutorError::AuthRequired(
            "Codex authentication required".to_string(),
        ));
    }

    let (thread_id, _) = super::Codex::start_or_fork_thread_with_linux_sandbox_fallback(
        thread_start_params,
        resume_session,
        sandbox,
        client.clone(),
    )
    .await?;

    client.register_session(&thread_id).await?;
    client.start_review(thread_id, review_target).await?;

    Ok(())
}
