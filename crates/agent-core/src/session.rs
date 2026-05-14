use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Pending,
    Streaming,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionEvent {
    pub state: SessionState,
    pub message: Option<String>,
    pub snapshot_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ArtifactSession {
    id: String,
    html: String,
    state: SessionState,
    message: Option<String>,
    snapshot_path: Option<PathBuf>,
}

impl ArtifactSession {
    pub fn new(id: String) -> Self {
        Self {
            id,
            html: String::new(),
            state: SessionState::Pending,
            message: None,
            snapshot_path: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn push_chunk(&mut self, chunk: &str) {
        self.state = SessionState::Streaming;
        self.html.push_str(chunk);
    }

    pub fn complete(&mut self, snapshot_path: Option<PathBuf>) {
        self.state = SessionState::Completed;
        self.snapshot_path = snapshot_path;
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = SessionState::Failed;
        self.message = Some(message.into());
    }

    pub fn full_html(&self) -> &str {
        &self.html
    }

    pub fn event(&self) -> SessionEvent {
        SessionEvent {
            state: self.state,
            message: self.message.clone(),
            snapshot_path: self
                .snapshot_path
                .as_ref()
                .map(|path| path.display().to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::session::{ArtifactSession, SessionState};

    #[test]
    fn artifact_session_tracks_status_chunks_and_snapshot() {
        let mut session = ArtifactSession::new("session-1".to_string());
        assert_eq!(session.event().state, SessionState::Pending);

        session.push_chunk("<!DOCTYPE html>");
        session.push_chunk("<html></html>");
        assert_eq!(session.event().state, SessionState::Streaming);
        assert_eq!(session.full_html(), "<!DOCTYPE html><html></html>");

        session.complete(Some(PathBuf::from(".t2w/artifacts/final.html")));
        let event = session.event();
        assert_eq!(event.state, SessionState::Completed);
        assert_eq!(
            event.snapshot_path.as_deref(),
            Some(".t2w/artifacts/final.html")
        );
    }
}
