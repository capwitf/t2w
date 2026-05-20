use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInput {
    pub stdin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOptions {
    pub persist_snapshot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRequest {
    pub instruction: String,
    pub input: SessionInput,
    pub template_id: String,
    pub skill_ids: Vec<String>,
    pub provider: String,
    pub model: String,
    pub options: RunOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Draft,
    Active,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Canceled,
}

impl RunStatus {
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Starting,
    Streaming,
    Finalizing,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudioSession {
    pub id: String,
    pub title: String,
    pub status: SessionStatus,
    #[serde(flatten)]
    pub request: RunRequest,
    pub latest_run_id: Option<String>,
    pub active_run_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl StudioSession {
    pub fn new(id: String, title: String, request: RunRequest, timestamp: String) -> Self {
        Self {
            id,
            title,
            status: SessionStatus::Draft,
            request,
            latest_run_id: None,
            active_run_id: None,
            created_at: timestamp.clone(),
            updated_at: timestamp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunAttempt {
    pub id: String,
    pub session_id: String,
    pub attempt: u32,
    pub status: RunStatus,
    pub phase: RunPhase,
    pub request: RunRequest,
    pub live_url: String,
    pub artifact_url: String,
    pub events_url: String,
    pub snapshot_path: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDescriptor {
    pub name: String,
    pub description: String,
}

pub fn default_templates() -> Vec<TemplateDescriptor> {
    vec![
        TemplateDescriptor {
            id: "table-explorer".to_string(),
            name: "Table Explorer".to_string(),
            description: "High-density structured table artifact".to_string(),
        },
        TemplateDescriptor {
            id: "incident-summary".to_string(),
            name: "Incident Summary".to_string(),
            description: "Narrative incident report with key status callouts".to_string(),
        },
        TemplateDescriptor {
            id: "kpi-dashboard".to_string(),
            name: "KPI Dashboard".to_string(),
            description: "Metric-forward dashboard for compact operational summaries".to_string(),
        },
        TemplateDescriptor {
            id: "timeline-report".to_string(),
            name: "Timeline Report".to_string(),
            description: "Chronological report optimized for event progression".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        RunOptions, RunPhase, RunRequest, RunStatus, SessionInput, SessionStatus, StudioSession,
        default_templates,
    };

    #[test]
    fn studio_session_starts_without_runs() {
        let session = StudioSession::new(
            "session-1".to_string(),
            "Production Cluster Logs".to_string(),
            RunRequest {
                instruction: "Build a log artifact".to_string(),
                input: SessionInput {
                    stdin: "ERROR boom".to_string(),
                },
                template_id: "table-explorer".to_string(),
                skill_ids: vec!["log-dashboard".to_string()],
                provider: "mock".to_string(),
                model: "mock-model".to_string(),
                options: RunOptions {
                    persist_snapshot: true,
                },
            },
            "2026-05-19T00:00:00Z".to_string(),
        );

        assert_eq!(session.id, "session-1");
        assert_eq!(session.status, SessionStatus::Draft);
        assert_eq!(session.latest_run_id, None);
        assert_eq!(session.active_run_id, None);
    }

    #[test]
    fn run_status_marks_terminal_states() {
        assert!(!RunStatus::Running.is_terminal());
        assert!(RunStatus::Completed.is_terminal());
        assert!(RunStatus::Failed.is_terminal());
        assert!(RunStatus::Canceled.is_terminal());
    }

    #[test]
    fn run_phase_keeps_execution_progress_separate_from_outcome() {
        assert_eq!(RunPhase::Starting, RunPhase::Starting);
        assert_ne!(RunPhase::Streaming, RunPhase::Finished);
    }

    #[test]
    fn default_templates_include_table_explorer() {
        let templates = default_templates();
        assert!(templates.iter().any(|template| {
            template.id == "table-explorer" && template.name == "Table Explorer"
        }));
    }
}
