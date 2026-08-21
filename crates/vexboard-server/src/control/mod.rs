pub mod docker;
pub mod systemd;

/// A lifecycle action to perform on a tracked service's backing unit or container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitAction {
    Start,
    Stop,
    Restart,
}

impl UnitAction {
    /// The audit-log action string for this operation, e.g. `"service.stop"`.
    pub fn audit_action(self) -> &'static str {
        match self {
            UnitAction::Start => "service.start",
            UnitAction::Stop => "service.stop",
            UnitAction::Restart => "service.restart",
        }
    }
}
