use super::UnitAction;

/// Write-side systemd1.Manager proxy. Kept separate from the read-only
/// `list_units` proxy in `probe/uptime.rs` — different concern, same interface.
#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManagerControl {
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
    fn restart_unit(&self, name: &str, mode: &str)
        -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

/// Start, stop, or restart a systemd unit via D-Bus. The queued job's
/// completion is intentionally not tracked here — systemd runs it
/// asynchronously, and the caller re-probes the service shortly after to
/// pick up the new state, matching how the rest of this app already learns
/// about state changes.
pub async fn control_unit(unit_name: &str, action: UnitAction) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;
    let proxy = SystemdManagerControlProxy::new(&conn).await?;
    const MODE: &str = "replace"; // systemd's default job mode, same as plain `systemctl`
    match action {
        UnitAction::Start => proxy.start_unit(unit_name, MODE).await?,
        UnitAction::Stop => proxy.stop_unit(unit_name, MODE).await?,
        UnitAction::Restart => proxy.restart_unit(unit_name, MODE).await?,
    };
    Ok(())
}

/// A line stream that owns the `journalctl` child process producing it.
/// `_child` is never read — it exists purely so the process stays alive
/// (and `kill_on_drop` fires) exactly as long as something is polling
/// `lines`, with no separate process-tracking table needed.
struct ChildLogStream {
    _child: tokio::process::Child,
    lines: tokio_stream::wrappers::LinesStream<tokio::io::BufReader<tokio::process::ChildStdout>>,
}

impl tokio_stream::Stream for ChildLogStream {
    type Item = std::io::Result<String>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.lines).poll_next(cx)
    }
}

/// Tails a systemd unit's journal (last 50 lines, then follows) by
/// spawning `journalctl -f`. The process is killed automatically when the
/// returned stream is dropped (an abandoned connection can't leak it).
pub async fn tail_unit_logs(
    unit_name: &str,
) -> anyhow::Result<impl tokio_stream::Stream<Item = std::io::Result<String>>> {
    use tokio::io::AsyncBufReadExt;

    let mut child = tokio::process::Command::new("journalctl")
        .args([
            "-u",
            unit_name,
            "-n",
            "50",
            "-f",
            "--no-pager",
            "-o",
            "short-iso",
        ])
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture journalctl stdout"))?;
    let lines = tokio_stream::wrappers::LinesStream::new(tokio::io::BufReader::new(stdout).lines());

    Ok(ChildLogStream {
        _child: child,
        lines,
    })
}
