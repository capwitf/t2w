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
pub struct FormulaStep {
    pub id: String,
    pub title: String,
    pub description: String,
    pub rules: Vec<String>,
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
    pub formula: Vec<FormulaStep>,
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
            formula: default_reinforcement_formula(),
        },
        TemplateDescriptor {
            id: "incident-summary".to_string(),
            name: "Incident Summary".to_string(),
            description: "Narrative incident report with key status callouts".to_string(),
            formula: default_reinforcement_formula(),
        },
        TemplateDescriptor {
            id: "kpi-dashboard".to_string(),
            name: "KPI Dashboard".to_string(),
            description: "Metric-forward dashboard for compact operational summaries".to_string(),
            formula: default_reinforcement_formula(),
        },
        TemplateDescriptor {
            id: "timeline-report".to_string(),
            name: "Timeline Report".to_string(),
            description: "Chronological report optimized for event progression".to_string(),
            formula: default_reinforcement_formula(),
        },
    ]
}

pub fn default_reinforcement_formula() -> Vec<FormulaStep> {
    vec![
        FormulaStep {
            id: "prompt".to_string(),
            title: "Prompt Formula".to_string(),
            description: "Stabilize role, input shape, and the output contract before generation."
                .to_string(),
            rules: vec![
                "State the role and the exact artifact type.".to_string(),
                "Repeat the output contract in one short line.".to_string(),
                "Keep visual rules in the prompt body, not in ad hoc follow-up text.".to_string(),
            ],
        },
        FormulaStep {
            id: "data".to_string(),
            title: "Data Formula".to_string(),
            description: "Normalize stdin and surface only the data that matters to the artifact."
                .to_string(),
            rules: vec![
                "Trim noise and keep the smallest useful sample.".to_string(),
                "Preserve identifiers, counts, and timestamps when they carry meaning.".to_string(),
                "Prefer structured excerpts over raw blobs when both exist.".to_string(),
            ],
        },
        FormulaStep {
            id: "theme".to_string(),
            title: "Theme Formula".to_string(),
            description: "Use a stable layout, density, typography, and accent system.".to_string(),
            rules: vec![
                "Keep the preview legible at the first glance.".to_string(),
                "Use one accent color for active state and keep the rest restrained.".to_string(),
                "Maintain a fixed shell around the artifact canvas.".to_string(),
            ],
        },
        FormulaStep {
            id: "run".to_string(),
            title: "Run Formula".to_string(),
            description: "Score the output, spot weak sections, and repair the HTML before finish."
                .to_string(),
            rules: vec![
                "Render streaming output incrementally and validate the final HTML shell."
                    .to_string(),
                "Flag gaps in content density, hierarchy, or contrast.".to_string(),
                "Finish with a snapshot that can be reopened locally.".to_string(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        RunOptions, RunPhase, RunRequest, RunStatus, SessionInput, SessionStatus, StudioSession,
        default_reinforcement_formula, default_templates,
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

    #[test]
    fn default_templates_include_reinforcement_formula() {
        let templates = default_templates();
        let template = templates
            .iter()
            .find(|template| template.id == "table-explorer")
            .expect("table explorer template");
        let stage_ids = template
            .formula
            .iter()
            .map(|stage| stage.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(stage_ids, vec!["prompt", "data", "theme", "run"]);
        assert!(template.formula.iter().all(|stage| !stage.rules.is_empty()));
    }

    #[test]
    fn default_reinforcement_formula_has_four_stages() {
        let formula = default_reinforcement_formula();
        let titles = formula
            .iter()
            .map(|stage| stage.title.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            titles,
            vec![
                "Prompt Formula",
                "Data Formula",
                "Theme Formula",
                "Run Formula"
            ]
        );
    }
}
