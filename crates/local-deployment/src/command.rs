use command_group::AsyncGroupChild;
#[cfg(unix)]
use nix::{
    sys::signal::{Signal, killpg},
    unistd::{Pid, getpgid},
};
use services::services::container::ContainerError;
#[cfg(unix)]
use tokio::time::Duration;

/// Overall timeout for kill_process_group to prevent indefinite hangs
#[cfg(unix)]
const KILL_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn kill_process_group(child: &mut AsyncGroupChild) -> Result<(), ContainerError> {
    #[cfg(unix)]
    {
        // Wrap the signal-sending logic in a timeout to prevent indefinite hangs
        let kill_result = tokio::time::timeout(KILL_TIMEOUT, kill_process_group_inner(child)).await;

        if kill_result.is_err() {
            tracing::warn!(
                timeout_secs = KILL_TIMEOUT.as_secs(),
                "kill_process_group timed out - forcing immediate kill"
            );
        }
    }

    // Always ensure we try to kill and wait, even after timeout
    let _ = child.kill().await;
    let _ = child.wait().await;
    Ok(())
}

#[cfg(unix)]
async fn kill_process_group_inner(child: &mut AsyncGroupChild) -> Result<(), ContainerError> {
    if let Some(pid) = child.inner().id() {
        let pgid = getpgid(Some(Pid::from_raw(pid as i32)))
            .map_err(|e| ContainerError::KillFailed(std::io::Error::other(e)))?;

        for sig in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGKILL] {
            if let Err(e) = killpg(pgid, sig) {
                tracing::warn!(
                    "Failed to send signal {:?} to process group {}: {}",
                    sig,
                    pgid,
                    e
                );
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            if child
                .inner()
                .try_wait()
                .map_err(ContainerError::Io)?
                .is_some()
            {
                break;
            }
        }
    }
    Ok(())
}
