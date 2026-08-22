use bollard::container::LogsOptions;
use bollard::Docker;
use tokio_stream::StreamExt;

use super::UnitAction;

/// Start, stop, or restart a Docker/Podman container by name via its Unix socket.
/// Every call passes `None` for options (default timeouts) rather than
/// constructing version-specific option structs.
pub async fn control_container(
    socket: &str,
    container_name: &str,
    action: UnitAction,
) -> anyhow::Result<()> {
    let docker = Docker::connect_with_socket(socket, 10, bollard::API_DEFAULT_VERSION)?;
    match action {
        UnitAction::Start => {
            docker
                .start_container::<String>(container_name, None)
                .await?;
        }
        UnitAction::Stop => {
            docker.stop_container(container_name, None).await?;
        }
        UnitAction::Restart => {
            docker.restart_container(container_name, None).await?;
        }
    }
    Ok(())
}

/// Tails a container's stdout/stderr (last 50 lines, then follows) via
/// bollard's native log-streaming API — no CLI subprocess needed, since
/// bollard already talks to the daemon directly for control actions above.
pub async fn tail_container_logs(
    socket: &str,
    container_name: &str,
) -> anyhow::Result<impl tokio_stream::Stream<Item = std::io::Result<String>>> {
    let docker = Docker::connect_with_socket(socket, 10, bollard::API_DEFAULT_VERSION)?;
    let options = LogsOptions::<String> {
        follow: true,
        stdout: true,
        stderr: true,
        tail: "50".to_string(),
        ..Default::default()
    };
    let stream = docker.logs(container_name, Some(options)).map(|item| {
        item.map(|log| log.to_string())
            .map_err(|e| std::io::Error::other(e.to_string()))
    });
    Ok(stream)
}
