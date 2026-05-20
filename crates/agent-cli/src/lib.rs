use std::collections::BinaryHeap;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

use agent_core::artifact_shell::{
    ArtifactShellContext, artifact_title_from_instruction, render_artifact_shell,
    render_artifact_shell_stream_chunk, render_artifact_shell_stream_finish,
    render_artifact_shell_stream_start,
};
use agent_core::prompt::{ContextSnapshot, InjectedSkill, build_prompt_request};
use agent_core::provider::{
    AnthropicConfig, AnthropicProvider, LlmProvider, MockProvider, ProviderError,
};
use agent_server::PreviewServer;
use agent_skills::{SkillDocument, SkillSelectionRequest, discover_skills, select_skills};
use anyhow::{Context, Result, anyhow};
use clap::{Parser, ValueEnum};
use futures::StreamExt;
use serde::Deserialize;
use time::OffsetDateTime;
use time::macros::format_description;
use tokio::time::{Duration, sleep};
use tracing::Level;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

pub mod studio;

#[derive(Debug, Clone, Parser)]
#[command(name = "t2w")]
pub struct CliArgs {
    pub instruction: String,
    #[arg(long, value_enum, default_value_t = ProviderKind::Anthropic)]
    pub provider: ProviderKind,
    #[arg(long = "skill")]
    pub skills: Vec<String>,
    #[arg(long = "skills-dir")]
    pub skills_dirs: Vec<PathBuf>,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long = "artifacts-dir")]
    pub artifacts_dir: Option<PathBuf>,
    #[arg(long = "no-open", default_value_t = false)]
    pub no_open: bool,
    #[arg(long = "no-snapshot", default_value_t = false)]
    pub no_snapshot: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProviderKind {
    Anthropic,
    Mock,
}

pub trait BrowserLauncher: Send + Sync {
    fn open(&self, url: &str) -> Result<()>;
}

pub struct SystemBrowserLauncher;

impl BrowserLauncher for SystemBrowserLauncher {
    fn open(&self, url: &str) -> Result<()> {
        webbrowser::open(url)
            .map(|_| ())
            .map_err(|error| anyhow!("failed to open browser for {url}: {error}"))
    }
}

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub live_url: String,
    pub snapshot_path: Option<PathBuf>,
    pub final_html: String,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    artifacts_dir: Option<PathBuf>,
    anthropic_api_key: Option<String>,
    anthropic_model: Option<String>,
    mock_chunk_delay_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct ResolvedConfig {
    artifacts_dir: PathBuf,
    anthropic_api_key: Option<String>,
    anthropic_model: String,
    mock_chunk_delay_ms: u64,
}

pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::from_default_env().add_directive(Level::INFO.into()));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

pub async fn run_with_callback<F>(
    args: CliArgs,
    launcher: &dyn BrowserLauncher,
    mut notify: F,
) -> Result<RunSummary>
where
    F: FnMut(&str),
{
    let cwd = env::current_dir().context("failed to resolve current directory")?;
    let config = resolve_config(&args, &cwd)?;
    let stdin = read_stdin()?;
    let selected_skills = load_selected_skills(&cwd, &args)?;
    let injected_skills = selected_skills
        .iter()
        .map(|skill| InjectedSkill {
            name: skill.metadata.name.clone(),
            instructions: skill.body.clone(),
        })
        .collect::<Vec<_>>();
    let workspace_files = collect_workspace_files(&cwd)?;
    let session_id = Uuid::new_v4().to_string();
    let shell_context = build_cli_artifact_shell_context(&args, &stdin, &config);

    let prompt = build_prompt_request(
        &ContextSnapshot {
            instruction: args.instruction.clone(),
            stdin: stdin.clone(),
            working_directory: cwd.clone(),
            workspace_files,
            injected_skills,
        },
        config.anthropic_model.clone(),
    );

    let server = PreviewServer::start().await?;
    let session = server.create_session(session_id.clone()).await?;
    let live_url = server.live_url(&session_id);
    notify(&format!("Live preview: {live_url}"));

    let execution_result: Result<RunSummary> = async {
        if !args.no_open {
            launcher.open(&live_url)?;
        }

        let provider = build_provider(args.provider, &config)?;
        let mut stream = provider.stream_html(prompt);
        let mut artifact_html = String::new();

        session
            .push_html(&render_artifact_shell_stream_start(&shell_context))
            .await;

        while let Some(next) = stream.next().await {
            let chunk = next.map_err(normalize_provider_error)?;
            artifact_html.push_str(&chunk);
            session
                .push_html(&render_artifact_shell_stream_chunk(&chunk))
                .await;
        }

        session
            .push_html(&render_artifact_shell_stream_finish())
            .await;
        let final_html = render_artifact_shell(&shell_context, &artifact_html);

        let snapshot_path = if args.no_snapshot {
            None
        } else {
            Some(save_snapshot(
                &config.artifacts_dir,
                &session_id,
                &final_html,
            )?)
        };

        session.complete(snapshot_path.clone()).await;
        Ok(RunSummary {
            live_url: live_url.clone(),
            snapshot_path,
            final_html,
        })
    }
    .await;

    if let Err(error) = &execution_result {
        session.fail(format!("{error:#}")).await;
        sleep(Duration::from_millis(25)).await;
    }

    let shutdown_result = server.shutdown().await;
    match (execution_result, shutdown_result) {
        (Ok(summary), Ok(())) => Ok(summary),
        (Ok(_), Err(shutdown_error)) => Err(shutdown_error),
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(shutdown_error)) => Err(error.context(format!(
            "preview server shutdown also failed: {shutdown_error:#}"
        ))),
    }
}

fn resolve_config(args: &CliArgs, cwd: &Path) -> Result<ResolvedConfig> {
    resolve_config_inputs(args.config.as_ref(), args.artifacts_dir.clone(), cwd)
}

fn resolve_config_inputs(
    config_path: Option<&PathBuf>,
    artifacts_dir_override: Option<PathBuf>,
    cwd: &Path,
) -> Result<ResolvedConfig> {
    let file_config = match config_path {
        Some(path) => {
            let contents = fs::read_to_string(path)
                .with_context(|| format!("failed to read config file {}", path.display()))?;
            toml::from_str::<FileConfig>(&contents)
                .with_context(|| format!("failed to parse config file {}", path.display()))?
        }
        None => FileConfig::default(),
    };

    let artifacts_dir = artifacts_dir_override
        .or_else(|| env::var_os("T2W_ARTIFACTS_DIR").map(PathBuf::from))
        .or(file_config.artifacts_dir)
        .unwrap_or_else(|| cwd.join(".t2w").join("artifacts"));

    let anthropic_api_key = env::var("T2W_ANTHROPIC_API_KEY")
        .ok()
        .or(file_config.anthropic_api_key);
    let anthropic_model = env::var("T2W_ANTHROPIC_MODEL")
        .ok()
        .or(file_config.anthropic_model)
        .unwrap_or_else(|| "claude-sonnet-4-5".to_string());
    let mock_chunk_delay_ms = env::var("T2W_MOCK_CHUNK_DELAY_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .or(file_config.mock_chunk_delay_ms)
        .unwrap_or(0);

    Ok(ResolvedConfig {
        artifacts_dir,
        anthropic_api_key,
        anthropic_model,
        mock_chunk_delay_ms,
    })
}

fn read_stdin() -> Result<String> {
    if io::stdin().is_terminal() {
        return Ok(String::new());
    }

    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .context("failed to read stdin")?;
    Ok(buffer)
}

fn load_selected_skills(cwd: &Path, args: &CliArgs) -> Result<Vec<SkillDocument>> {
    let mut roots = vec![cwd.join("skills"), cwd.join(".t2w").join("skills")];
    roots.extend(args.skills_dirs.iter().cloned());

    let catalog = discover_skills(&roots)?;
    Ok(select_skills(
        &SkillSelectionRequest {
            explicit_skills: args.skills.clone(),
            user_input: args.instruction.clone(),
        },
        &catalog.skills,
    )?)
}

fn collect_workspace_files(cwd: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(cwd)
        .into_iter()
        .filter_entry(|entry| should_descend(entry, cwd))
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path == cwd {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }

        let relative = path.strip_prefix(cwd).unwrap_or(path);
        files.push(relative.display().to_string());
    }

    Ok(sort_and_limit_workspace_files(files))
}

fn sort_and_limit_workspace_files(mut files: Vec<String>) -> Vec<String> {
    let mut smallest = BinaryHeap::new();

    for file in files.drain(..) {
        smallest.push(file);
        if smallest.len() > 50 {
            smallest.pop();
        }
    }

    smallest.into_sorted_vec()
}

fn should_descend(entry: &DirEntry, root: &Path) -> bool {
    if entry.path() == root {
        return true;
    }

    !matches!(entry.file_name().to_str(), Some(".git" | "target" | ".t2w"))
}

fn build_provider(kind: ProviderKind, config: &ResolvedConfig) -> Result<Box<dyn LlmProvider>> {
    match kind {
        ProviderKind::Anthropic => {
            let api_key = config
                .anthropic_api_key
                .clone()
                .ok_or_else(|| anyhow!("missing T2W_ANTHROPIC_API_KEY for Anthropic provider"))?;
            let client = reqwest::Client::builder().build()?;
            let anthropic = AnthropicProvider::new(
                client,
                AnthropicConfig::new(api_key, config.anthropic_model.clone()),
            );
            Ok(Box::new(anthropic))
        }
        ProviderKind::Mock => Ok(Box::new(
            MockProvider::new(default_mock_chunks()).with_delay(config.mock_chunk_delay_ms),
        )),
    }
}

fn build_cli_artifact_shell_context(
    args: &CliArgs,
    stdin: &str,
    config: &ResolvedConfig,
) -> ArtifactShellContext {
    ArtifactShellContext::new(
        artifact_title_from_instruction(&args.instruction),
        args.instruction.clone(),
        stdin.to_string(),
        "Artifact Console",
        "Default CLI artifact shell with prompt, data, and run controls.",
        provider_label(args.provider),
        model_label(args.provider, config),
    )
}

fn provider_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::Mock => "mock",
    }
}

fn model_label(kind: ProviderKind, config: &ResolvedConfig) -> String {
    match kind {
        ProviderKind::Anthropic => config.anthropic_model.clone(),
        ProviderKind::Mock => "mock-model".to_string(),
    }
}

fn default_mock_chunks() -> Vec<String> {
    vec![
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Mock Artifact</title></head>"
            .to_string(),
        "<body><main><h1>Mock Artifact</h1><p>Streaming preview is active.</p>".to_string(),
        "<section><p>This document came from the mock provider.</p></section></main></body></html>"
            .to_string(),
    ]
}

fn save_snapshot(artifacts_dir: &Path, session_id: &str, html: &str) -> Result<PathBuf> {
    fs::create_dir_all(artifacts_dir).with_context(|| {
        format!(
            "failed to create artifacts directory {}",
            artifacts_dir.display()
        )
    })?;
    let timestamp = OffsetDateTime::now_utc().format(&format_description!(
        "[year][month][day]T[hour][minute][second]Z"
    ))?;
    let path = artifacts_dir.join(format!("{timestamp}-{session_id}.html"));
    fs::write(&path, html)
        .with_context(|| format!("failed to write snapshot {}", path.display()))?;
    Ok(path)
}

fn normalize_provider_error(error: ProviderError) -> anyhow::Error {
    anyhow!(error)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::{env, fs};

    use super::{BrowserLauncher, CliArgs, ProviderKind, resolve_config, save_snapshot};

    struct RecordingLauncher {
        opened: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingLauncher {
        fn new() -> Self {
            Self {
                opened: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl BrowserLauncher for RecordingLauncher {
        fn open(&self, url: &str) -> anyhow::Result<()> {
            self.opened.lock().unwrap().push(url.to_string());
            Ok(())
        }
    }

    #[test]
    fn resolve_config_prefers_env_over_file() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("t2w.toml");
        fs::write(
            &config_path,
            "anthropic_model = 'file-model'\nmock_chunk_delay_ms = 12\n",
        )
        .unwrap();
        unsafe {
            env::set_var("T2W_ANTHROPIC_MODEL", "env-model");
        }

        let config = resolve_config(
            &CliArgs {
                instruction: "demo".to_string(),
                provider: ProviderKind::Mock,
                skills: Vec::new(),
                skills_dirs: Vec::new(),
                config: Some(config_path),
                artifacts_dir: None,
                no_open: true,
                no_snapshot: false,
            },
            temp.path(),
        )
        .unwrap();

        assert_eq!(config.anthropic_model, "env-model");
        assert_eq!(config.mock_chunk_delay_ms, 12);

        unsafe {
            env::remove_var("T2W_ANTHROPIC_MODEL");
        }
    }

    #[test]
    fn save_snapshot_writes_html_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = save_snapshot(temp.path(), "abc", "<!DOCTYPE html>").unwrap();

        assert!(path.exists());
        assert_eq!(fs::read_to_string(path).unwrap(), "<!DOCTYPE html>");
    }

    #[test]
    fn launcher_records_urls() {
        let launcher = RecordingLauncher::new();
        launcher.open("http://127.0.0.1/live/demo").unwrap();

        let opened = launcher.opened.lock().unwrap().clone();
        assert_eq!(opened, vec!["http://127.0.0.1/live/demo"]);
    }

    #[test]
    fn collect_workspace_files_skips_nested_git_target_and_t2w_trees() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".git").join("objects")).unwrap();
        fs::create_dir_all(temp.path().join("target").join("debug")).unwrap();
        fs::create_dir_all(temp.path().join(".t2w").join("artifacts")).unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join(".git").join("objects").join("secret"), "x").unwrap();
        fs::write(
            temp.path().join("target").join("debug").join("build.log"),
            "x",
        )
        .unwrap();
        fs::write(
            temp.path()
                .join(".t2w")
                .join("artifacts")
                .join("artifact.html"),
            "x",
        )
        .unwrap();
        fs::write(temp.path().join("src").join("main.rs"), "fn main() {}").unwrap();

        let files = super::collect_workspace_files(temp.path()).unwrap();

        assert_eq!(files, vec!["src\\main.rs"]);
    }

    #[test]
    fn sort_and_limit_workspace_files_returns_lexicographically_smallest_50() {
        let files = (0..60)
            .rev()
            .map(|index| format!("file-{index:02}.txt"))
            .collect::<Vec<_>>();

        let files = super::sort_and_limit_workspace_files(files);

        assert_eq!(files.len(), 50);
        assert_eq!(files.first().map(String::as_str), Some("file-00.txt"));
        assert_eq!(files.last().map(String::as_str), Some("file-49.txt"));
    }

    #[tokio::test]
    async fn anthropic_failure_shuts_down_preview_server() {
        let args = CliArgs {
            instruction: "demo".to_string(),
            provider: ProviderKind::Anthropic,
            skills: Vec::new(),
            skills_dirs: Vec::new(),
            config: None,
            artifacts_dir: None,
            no_open: true,
            no_snapshot: false,
        };
        let launcher = RecordingLauncher::new();
        let live_url = Arc::new(Mutex::new(None::<String>));
        let live_url_capture = Arc::clone(&live_url);

        let result = super::run_with_callback(args, &launcher, |message| {
            if let Some(url) = message.strip_prefix("Live preview: ") {
                *live_url_capture.lock().unwrap() = Some(url.to_string());
            }
        })
        .await;

        assert!(result.is_err());

        let live_url = live_url.lock().unwrap().clone().unwrap();
        let response = reqwest::get(live_url).await;
        assert!(response.is_err());
    }
}
