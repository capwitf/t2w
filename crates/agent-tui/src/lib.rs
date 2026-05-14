use agent_core::session::SessionEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiShell {
    title: String,
}

impl TuiShell {
    pub fn new() -> Self {
        Self {
            title: "t2w live preview".to_string(),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn render_status_line(&self, event: &SessionEvent) -> String {
        let state = match event.state {
            agent_core::session::SessionState::Pending => "pending",
            agent_core::session::SessionState::Streaming => "streaming",
            agent_core::session::SessionState::Completed => "completed",
            agent_core::session::SessionState::Failed => "failed",
        };
        match &event.snapshot_path {
            Some(path) => format!("{} | {} | snapshot {}", self.title, state, path),
            None => format!("{} | {}", self.title, state),
        }
    }
}

impl Default for TuiShell {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use agent_core::session::{SessionEvent, SessionState};

    use crate::TuiShell;

    #[test]
    fn render_status_line_shows_snapshot_when_available() {
        let shell = TuiShell::new();
        let line = shell.render_status_line(&SessionEvent {
            state: SessionState::Completed,
            message: None,
            snapshot_path: Some(".t2w/artifacts/final.html".to_string()),
        });

        assert!(line.contains("completed"));
        assert!(line.contains("final.html"));
    }
}
