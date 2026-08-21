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
