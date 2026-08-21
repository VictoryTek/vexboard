use bollard::Docker;

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
