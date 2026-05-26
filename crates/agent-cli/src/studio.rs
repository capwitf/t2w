use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use agent_core::artifact_shell::{
    ArtifactShellContext, artifact_title_from_instruction, render_artifact_shell,
    render_artifact_shell_stream_chunk, render_artifact_shell_stream_finish,
    render_artifact_shell_stream_start,
};
use agent_core::prompt::{ContextSnapshot, InjectedSkill, build_prompt_request};
use agent_core::provider::{AnthropicConfig, AnthropicProvider, LlmProvider, MockProvider};
use agent_core::studio::{
    RunRequest, SkillDescriptor, default_reinforcement_formula, default_templates,
};
use agent_server::studio::{RunExecutionContext, RunExecutor, StudioServer, StudioServerConfig};
use agent_skills::{SkillSelectionRequest, discover_skills, select_skills};
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use futures::StreamExt;

use crate::{
    ResolvedConfig, collect_workspace_files, default_mock_chunks, normalize_provider_error,
    resolve_config_inputs, save_snapshot,
};

#[derive(Debug, Clone, Parser)]
#[command(name = "t2w-studio")]
pub struct StudioArgs {
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    #[arg(long, default_value_t = 3000)]
    pub port: u16,
    #[arg(long = "skills-dir")]
    pub skills_dirs: Vec<PathBuf>,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long = "artifacts-dir")]
    pub artifacts_dir: Option<PathBuf>,
    #[arg(long = "sessions-file")]
    pub sessions_file: Option<PathBuf>,
}

pub async fn start_studio_server(args: StudioArgs) -> Result<StudioServer> {
    let cwd = env::current_dir().context("failed to resolve current directory")?;
    let config = resolve_config_inputs(args.config.as_ref(), args.artifacts_dir.clone(), &cwd)?;
    let skill_roots = skill_roots(&cwd, &args.skills_dirs);
    let skills = summarize_skills(&skill_roots)?;
    let storage_path = args
        .sessions_file
        .clone()
        .unwrap_or_else(|| cwd.join(".t2w").join("studio-state.json"));
    let executor = build_studio_executor(cwd, skill_roots, config);

    StudioServer::start(StudioServerConfig {
        bind_addr: build_bind_addr(&args),
        skills,
        executor,
        storage_path: Some(storage_path),
    })
    .await
}

pub fn build_bind_addr(args: &StudioArgs) -> String {
    format!("{}:{}", args.host, args.port)
}

pub fn studio_run_request(request: &RunRequest) -> RunRequest {
    request.clone()
}

pub fn summarize_skills(roots: &[PathBuf]) -> Result<Vec<SkillDescriptor>> {
    let catalog = discover_skills(roots)?;
    Ok(catalog
        .skills
        .into_iter()
        .map(|skill| SkillDescriptor {
            name: skill.metadata.name,
            description: skill.metadata.description,
        })
        .collect())
}

fn skill_roots(cwd: &std::path::Path, extra_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = vec![cwd.join("skills"), cwd.join(".t2w").join("skills")];
    roots.extend(extra_roots.iter().cloned());
    roots
}

fn build_studio_executor(
    cwd: PathBuf,
    skill_roots: Vec<PathBuf>,
    config: ResolvedConfig,
) -> RunExecutor {
    Arc::new(move |context: RunExecutionContext| {
        let cwd = cwd.clone();
        let skill_roots = skill_roots.clone();
        let config = config.clone();

        Box::pin(async move { execute_run(context, &cwd, &skill_roots, &config).await })
    })
}

async fn execute_run(
    context: RunExecutionContext,
    cwd: &std::path::Path,
    skill_roots: &[PathBuf],
    config: &ResolvedConfig,
) -> Result<()> {
    let request = studio_run_request(&context.run.request);
    let shell_context = build_studio_artifact_shell_context(&context.session.title, &request);
    let selected_skills = load_run_skills(skill_roots, &request)?;
    let mut injected_skills = vec![template_injected_skill(&request)];
    injected_skills.extend(
        selected_skills
            .iter()
            .map(|skill| InjectedSkill {
                name: skill.metadata.name.clone(),
                instructions: skill.body.clone(),
            })
            .collect::<Vec<_>>(),
    );
    let workspace_files = collect_workspace_files(cwd)?;

    let prompt = build_prompt_request(
        &ContextSnapshot {
            instruction: request.instruction.clone(),
            stdin: request.input.stdin.clone(),
            working_directory: cwd.to_path_buf(),
            workspace_files,
            injected_skills,
        },
        request.model.clone(),
    );

    let provider = build_provider_for_request(&request, config)?;
    let mut stream = provider.stream_html(prompt);
    let mut artifact_html = String::new();

    context
        .handle
        .push_html(&render_artifact_shell_stream_start(&shell_context))
        .await;

    while let Some(next) = stream.next().await {
        let chunk = next.map_err(normalize_provider_error)?;
        artifact_html.push_str(&chunk);
        context
            .handle
            .push_html(&render_artifact_shell_stream_chunk(&chunk))
            .await;
    }

    context
        .handle
        .push_html(&render_artifact_shell_stream_finish())
        .await;
    let final_html = render_artifact_shell(&shell_context, &artifact_html);

    let snapshot_path = if request.options.persist_snapshot {
        Some(save_snapshot(
            &config.artifacts_dir,
            &context.run.id,
            &final_html,
        )?)
    } else {
        None
    };

    context.handle.complete(snapshot_path).await;
    Ok(())
}

fn build_studio_artifact_shell_context(
    session_title: &str,
    request: &RunRequest,
) -> ArtifactShellContext {
    let template = default_templates()
        .into_iter()
        .find(|template| template.id == request.template_id);
    let (template_name, template_description, formula) = match template {
        Some(template) => (template.name, template.description, template.formula),
        None => (
            request.template_id.clone(),
            "Studio-selected artifact template.".to_string(),
            default_reinforcement_formula(),
        ),
    };
    let title = if session_title.trim().is_empty() {
        artifact_title_from_instruction(&request.instruction)
    } else {
        session_title.to_string()
    };

    ArtifactShellContext::new(
        title,
        request.instruction.clone(),
        request.input.stdin.clone(),
        template_name,
        template_description,
        request.provider.clone(),
        request.model.clone(),
    )
    .with_formula(formula)
}

fn load_run_skills(
    roots: &[PathBuf],
    request: &RunRequest,
) -> Result<Vec<agent_skills::SkillDocument>> {
    let catalog = discover_skills(roots)?;
    Ok(select_skills(
        &SkillSelectionRequest {
            explicit_skills: request.skill_ids.clone(),
            user_input: request.instruction.clone(),
        },
        &catalog.skills,
    )?)
}

fn build_provider_for_request(
    request: &RunRequest,
    config: &ResolvedConfig,
) -> Result<Box<dyn LlmProvider>> {
    match request.provider.to_ascii_lowercase().as_str() {
        "anthropic" => {
            if !config.anthropic_enabled {
                return Err(anyhow!(
                    "anthropic provider is disabled; set T2W_ENABLE_ANTHROPIC=1 to enable key-backed generation"
                ));
            }
            let api_key = config
                .anthropic_api_key
                .clone()
                .ok_or_else(|| anyhow!("missing T2W_ANTHROPIC_API_KEY for Anthropic provider"))?;
            let client = reqwest::Client::builder().build()?;
            let anthropic = AnthropicProvider::new(
                client,
                AnthropicConfig::new(api_key, request.model.clone()),
            );
            Ok(Box::new(anthropic))
        }
        "mock" => Ok(Box::new(
            MockProvider::new(default_mock_chunks()).with_delay(config.mock_chunk_delay_ms),
        )),
        other => Err(anyhow!("unsupported provider for studio run: {other}")),
    }
}

fn template_injected_skill(request: &RunRequest) -> InjectedSkill {
    let template = default_templates()
        .into_iter()
        .find(|template| template.id == request.template_id);
    let instructions = match template {
        Some(template) => format!(
            "Use the {} template. {}",
            template.name, template.description
        ),
        None => format!(
            "Use the {} template selected by the studio.",
            request.template_id
        ),
    };

    InjectedSkill {
        name: format!("template:{}", request.template_id),
        instructions,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use agent_core::studio::RunRequest;

    use super::{
        StudioArgs, build_bind_addr, studio_run_request, summarize_skills, template_injected_skill,
    };

    #[test]
    fn build_bind_addr_combines_host_and_port() {
        let args = StudioArgs {
            host: "127.0.0.1".to_string(),
            port: 4040,
            skills_dirs: Vec::new(),
            config: None,
            artifacts_dir: None,
            sessions_file: None,
        };

        assert_eq!(build_bind_addr(&args), "127.0.0.1:4040");
    }

    #[test]
    fn summarize_skills_returns_name_and_description() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("skills").join("log-dashboard");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: log-dashboard
description: Turn logs into dashboards
---
# Log Dashboard
"#,
        )
        .unwrap();

        let skills = summarize_skills(&[temp.path().join("skills")]).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "log-dashboard");
        assert_eq!(skills[0].description, "Turn logs into dashboards");
    }

    #[test]
    fn studio_run_request_uses_session_payload_directly() {
        let request = RunRequest {
            instruction: "Build a timeline".to_string(),
            input: agent_core::studio::SessionInput {
                stdin: "line 1".to_string(),
            },
            template_id: "timeline-report".to_string(),
            skill_ids: vec!["incident-summary".to_string()],
            provider: "mock".to_string(),
            model: "mock-model".to_string(),
            options: agent_core::studio::RunOptions {
                persist_snapshot: false,
            },
        };

        let derived = studio_run_request(&request);

        assert_eq!(derived.template_id, "timeline-report");
        assert_eq!(derived.skill_ids, vec!["incident-summary"]);
        assert!(!derived.options.persist_snapshot);
    }

    #[test]
    fn template_injected_skill_keeps_reinforcement_formula_local() {
        let request = RunRequest {
            instruction: "Build a timeline".to_string(),
            input: agent_core::studio::SessionInput {
                stdin: "line 1".to_string(),
            },
            template_id: "timeline-report".to_string(),
            skill_ids: Vec::new(),
            provider: "mock".to_string(),
            model: "mock-model".to_string(),
            options: agent_core::studio::RunOptions {
                persist_snapshot: false,
            },
        };

        let injected = template_injected_skill(&request);

        assert_eq!(injected.name, "template:timeline-report");
        assert!(injected.instructions.contains("Timeline Report"));
        assert!(!injected.instructions.contains("Reinforcement Formula"));
        assert!(!injected.instructions.contains("Prompt Formula"));
        assert!(!injected.instructions.contains("Data Formula"));
        assert!(!injected.instructions.contains("Theme Formula"));
        assert!(!injected.instructions.contains("Run Formula"));
    }
}
