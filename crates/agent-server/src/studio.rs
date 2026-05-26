use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::{Path as FsPath, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use agent_core::studio::{
    RunAttempt, RunOptions, RunPhase, RunRequest, RunStatus, SessionInput, SessionStatus,
    SkillDescriptor, StudioSession, TemplateDescriptor, default_templates,
};
use anyhow::Result as AnyhowResult;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, serve};
use bytes::Bytes;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::macros::format_description;
use tokio::fs;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock, broadcast, oneshot};
use tokio::task::JoinHandle;
use tracing::error;
use uuid::Uuid;

pub type RunExecutionFuture = Pin<Box<dyn Future<Output = AnyhowResult<()>> + Send>>;
pub type RunExecutor = Arc<dyn Fn(RunExecutionContext) -> RunExecutionFuture + Send + Sync>;

const STUDIO_INDEX_HTML: &str = include_str!("../assets/studio/index.html");
const STUDIO_CSS: &str = include_str!("../assets/studio/studio.css");
const STUDIO_JS: &str = include_str!("../assets/studio/studio.js");

#[derive(Clone)]
pub struct RunExecutionContext {
    pub session: StudioSession,
    pub run: RunAttempt,
    pub handle: StudioRunHandle,
}

#[derive(Clone)]
pub struct StudioServerConfig {
    pub bind_addr: String,
    pub skills: Vec<SkillDescriptor>,
    pub executor: RunExecutor,
    pub storage_path: Option<PathBuf>,
}

#[derive(Clone)]
struct AppState {
    sessions: Arc<RwLock<HashMap<String, SessionRecord>>>,
    runs: Arc<RwLock<HashMap<String, SharedRun>>>,
    active_run: Arc<Mutex<Option<ActiveRun>>>,
    skills: Arc<Vec<SkillDescriptor>>,
    templates: Arc<Vec<TemplateDescriptor>>,
    executor: RunExecutor,
    storage_path: Option<PathBuf>,
}

struct ActiveRun {
    run_id: String,
    session_id: String,
    task: JoinHandle<()>,
}

struct SessionRecord {
    session: StudioSession,
    run_ids: Vec<String>,
}

type SharedRun = Arc<RwLock<RunEntry>>;

struct RunEntry {
    run: RunAttempt,
    html: String,
    html_tx: broadcast::Sender<String>,
    event_tx: broadcast::Sender<RunAttempt>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedStudioState {
    #[serde(default)]
    sessions: Vec<PersistedSessionRecord>,
    #[serde(default)]
    runs: Vec<PersistedRunRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedSessionRecord {
    session: StudioSession,
    run_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedRunRecord {
    run: RunAttempt,
    #[serde(default)]
    html: String,
}

struct LoadedStudioState {
    sessions: HashMap<String, SessionRecord>,
    runs: HashMap<String, SharedRun>,
}

#[derive(Clone)]
pub struct StudioRunHandle {
    state: AppState,
    run_id: String,
}

impl StudioRunHandle {
    pub async fn push_html(&self, chunk: &str) {
        if let Some(shared) = self.shared_run().await {
            let mut entry = shared.write().await;
            entry.run.status = RunStatus::Running;
            entry.run.phase = RunPhase::Streaming;
            entry.html.push_str(chunk);
            let _ = entry.html_tx.send(chunk.to_string());
            let _ = entry.event_tx.send(entry.run.clone());
        }
    }

    pub async fn complete(&self, snapshot_path: Option<PathBuf>) {
        self.finish_run(RunStatus::Completed, snapshot_path, None)
            .await;
    }

    pub async fn fail(&self, message: impl Into<String>) {
        self.finish_run(RunStatus::Failed, None, Some(message.into()))
            .await;
    }

    pub async fn cancel(&self, message: impl Into<String>) {
        self.finish_run(RunStatus::Canceled, None, Some(message.into()))
            .await;
    }

    pub async fn current_status(&self) -> Option<RunStatus> {
        let shared = self.shared_run().await?;
        Some(shared.read().await.run.status)
    }

    async fn current_run(&self) -> Option<RunAttempt> {
        let shared = self.shared_run().await?;
        Some(shared.read().await.run.clone())
    }

    async fn snapshot_and_subscribe(&self) -> Option<RunStreamSnapshot> {
        let shared = self.shared_run().await?;
        let entry = shared.read().await;
        Some(RunStreamSnapshot {
            html: entry.html.clone(),
            run: entry.run.clone(),
            html_rx: entry.html_tx.subscribe(),
            event_rx: entry.event_tx.subscribe(),
        })
    }

    async fn replay_from(&self, offset: usize) -> Option<RunReplay> {
        let shared = self.shared_run().await?;
        let entry = shared.read().await;
        Some(RunReplay {
            suffix: entry.html.get(offset..).unwrap_or("").to_string(),
            run: entry.run.clone(),
        })
    }

    async fn shared_run(&self) -> Option<SharedRun> {
        self.state.runs.read().await.get(&self.run_id).cloned()
    }

    async fn finish_run(
        &self,
        status: RunStatus,
        snapshot_path: Option<PathBuf>,
        error: Option<String>,
    ) {
        let Some(shared) = self.shared_run().await else {
            return;
        };

        let session_id = {
            let mut entry = shared.write().await;
            if entry.run.status.is_terminal() {
                return;
            }

            entry.run.phase = RunPhase::Finalizing;
            entry.run.status = status;
            entry.run.phase = RunPhase::Finished;
            entry.run.snapshot_path = snapshot_path
                .as_ref()
                .map(|path| path.display().to_string());
            entry.run.error = error;
            entry.run.finished_at = Some(now_string());
            let run_snapshot = entry.run.clone();
            let _ = entry.event_tx.send(run_snapshot.clone());
            entry.run.session_id.clone()
        };

        {
            let mut sessions = self.state.sessions.write().await;
            if let Some(record) = sessions.get_mut(&session_id) {
                if record.session.active_run_id.as_deref() == Some(self.run_id.as_str()) {
                    record.session.active_run_id = None;
                }
                record.session.status = session_status_from_run_status(status);
                record.session.updated_at = now_string();
            }
        }

        self.clear_active_run().await;
        log_persist_studio_state_error(&self.state).await;
    }

    async fn clear_active_run(&self) {
        let mut active = self.state.active_run.lock().await;
        if active.as_ref().map(|active_run| active_run.run_id.as_str())
            == Some(self.run_id.as_str())
        {
            active.take();
        }
    }
}

struct RunStreamSnapshot {
    html: String,
    run: RunAttempt,
    html_rx: broadcast::Receiver<String>,
    event_rx: broadcast::Receiver<RunAttempt>,
}

struct RunReplay {
    suffix: String,
    run: RunAttempt,
}

pub struct StudioServer {
    base_url: String,
    state: AppState,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: JoinHandle<AnyhowResult<()>>,
}

impl StudioServer {
    pub async fn start(config: StudioServerConfig) -> AnyhowResult<Self> {
        let listener = TcpListener::bind(&config.bind_addr).await?;
        let address = listener.local_addr()?;
        let base_url = format!("http://{address}");
        let storage_path = config.storage_path.clone();
        let persisted = load_persisted_state(storage_path.as_deref()).await?;
        let state = AppState {
            sessions: Arc::new(RwLock::new(persisted.sessions)),
            runs: Arc::new(RwLock::new(persisted.runs)),
            active_run: Arc::new(Mutex::new(None)),
            skills: Arc::new(config.skills),
            templates: Arc::new(default_templates()),
            executor: config.executor,
            storage_path,
        };
        let app = router(state.clone());
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server_task = tokio::spawn(async move {
            serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .map_err(Into::into)
        });

        Ok(Self {
            base_url,
            state,
            shutdown_tx: Some(shutdown_tx),
            server_task,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn shutdown(mut self) -> AnyhowResult<()> {
        abort_active_run(&self.state).await;
        log_persist_studio_state_error(&self.state).await;
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.server_task.await??;
        Ok(())
    }
}

async fn load_persisted_state(storage_path: Option<&FsPath>) -> AnyhowResult<LoadedStudioState> {
    let Some(storage_path) = storage_path else {
        return Ok(empty_loaded_state());
    };
    let contents = match fs::read_to_string(storage_path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(empty_loaded_state());
        }
        Err(error) => return Err(error.into()),
    };
    let persisted: PersistedStudioState = match serde_json::from_str(&contents) {
        Ok(persisted) => persisted,
        Err(error) => {
            preserve_corrupt_state_file(storage_path).await;
            error!(
                ?error,
                path = %storage_path.display(),
                "failed to parse persisted studio state; starting with empty state"
            );
            return Ok(empty_loaded_state());
        }
    };
    let mut sessions = HashMap::new();
    let mut runs = HashMap::new();
    let mut run_statuses = HashMap::new();

    for record in persisted.sessions {
        sessions.insert(
            record.session.id.clone(),
            SessionRecord {
                session: record.session,
                run_ids: record.run_ids,
            },
        );
    }

    for record in persisted.runs {
        let mut run = record.run;
        recover_non_terminal_run(&mut run);
        let run_id = run.id.clone();
        run_statuses.insert(run_id.clone(), run.status);
        let (html_tx, _) = broadcast::channel(64);
        let (event_tx, _) = broadcast::channel(64);
        let entry = Arc::new(RwLock::new(RunEntry {
            run,
            html: record.html,
            html_tx,
            event_tx,
        }));
        let run_snapshot = entry.read().await.run.clone();
        entry.read().await.event_tx.send(run_snapshot).ok();
        runs.insert(run_id, entry);
    }

    for record in sessions.values_mut() {
        recover_loaded_session(record, &run_statuses);
    }

    Ok(LoadedStudioState { sessions, runs })
}

fn empty_loaded_state() -> LoadedStudioState {
    LoadedStudioState {
        sessions: HashMap::new(),
        runs: HashMap::new(),
    }
}

fn recover_non_terminal_run(run: &mut RunAttempt) {
    if run.status.is_terminal() {
        return;
    }

    run.status = RunStatus::Canceled;
    run.phase = RunPhase::Finished;
    run.error = Some("server restarted during run".to_string());
    run.finished_at.get_or_insert_with(now_string);
}

fn recover_loaded_session(record: &mut SessionRecord, run_statuses: &HashMap<String, RunStatus>) {
    let active_run_is_recoverable = record
        .session
        .active_run_id
        .as_ref()
        .is_some_and(|run_id| run_statuses.get(run_id).is_some());
    if !active_run_is_recoverable {
        record.session.active_run_id = None;
    }

    if let Some(run_id) = record.session.active_run_id.as_ref()
        && run_statuses
            .get(run_id)
            .is_some_and(|status| status.is_terminal())
    {
        record.session.active_run_id = None;
    }

    if let Some(run_id) = record.session.latest_run_id.as_ref()
        && let Some(status) = run_statuses.get(run_id)
    {
        record.session.status = session_status_from_run_status(*status);
        return;
    }

    if record.session.active_run_id.is_none() && record.session.status == SessionStatus::Active {
        record.session.status = SessionStatus::Canceled;
    }
}

async fn preserve_corrupt_state_file(storage_path: &FsPath) {
    let mut corrupt_path = corrupt_state_path(storage_path);
    if fs::metadata(&corrupt_path).await.is_ok() {
        corrupt_path = corrupt_path.with_extension(format!("corrupt.{}", Uuid::new_v4()));
    }

    if let Err(error) = fs::rename(storage_path, &corrupt_path).await {
        error!(
            ?error,
            path = %storage_path.display(),
            corrupt_path = %corrupt_path.display(),
            "failed to preserve corrupt studio state file"
        );
    }
}

fn corrupt_state_path(storage_path: &FsPath) -> PathBuf {
    match storage_path.extension() {
        Some(extension) => {
            let mut corrupt_extension = extension.to_os_string();
            corrupt_extension.push(".corrupt");
            storage_path.with_extension(corrupt_extension)
        }
        None => storage_path.with_extension("corrupt"),
    }
}

async fn write_file_atomically(storage_path: &FsPath, contents: String) -> AnyhowResult<()> {
    let parent = storage_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| FsPath::new("."));
    fs::create_dir_all(parent).await?;

    let file_name = storage_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "studio-state.json".to_string());
    let temp_path = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));

    fs::write(&temp_path, contents).await?;
    match fs::rename(&temp_path, storage_path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if fs::metadata(storage_path).await.is_ok() {
                fs::remove_file(storage_path).await?;
            }
            fs::rename(&temp_path, storage_path).await?;
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&temp_path).await;
            Err(error.into())
        }
    }
}

async fn persist_studio_state(state: &AppState) -> AnyhowResult<()> {
    let Some(storage_path) = state.storage_path.as_ref() else {
        return Ok(());
    };

    let mut sessions = state
        .sessions
        .read()
        .await
        .values()
        .map(|record| PersistedSessionRecord {
            session: record.session.clone(),
            run_ids: record.run_ids.clone(),
        })
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| left.session.id.cmp(&right.session.id));

    let mut runs = Vec::new();
    for shared in state.runs.read().await.values() {
        let entry = shared.read().await;
        runs.push(PersistedRunRecord {
            run: entry.run.clone(),
            html: entry.html.clone(),
        });
    }
    runs.sort_by(|left, right| left.run.id.cmp(&right.run.id));

    let persisted = PersistedStudioState { sessions, runs };
    let contents = serde_json::to_string_pretty(&persisted)?;
    write_file_atomically(storage_path, contents).await?;
    Ok(())
}

async fn persist_studio_state_or_api_error(state: &AppState) -> std::result::Result<(), ApiError> {
    persist_studio_state(state)
        .await
        .map_err(|error| ApiError::internal(format!("failed to persist studio state: {error:#}")))
}

async fn log_persist_studio_state_error(state: &AppState) {
    if let Err(error) = persist_studio_state(state).await {
        error!(?error, "failed to persist studio state");
    }
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(studio_shell))
        .route("/studio.css", get(studio_stylesheet))
        .route("/studio.js", get(studio_javascript))
        .route("/sessions", post(create_session).get(list_sessions))
        .route(
            "/sessions/{session_id}",
            get(get_session).patch(update_session),
        )
        .route(
            "/sessions/{session_id}/runs",
            post(create_run).get(list_runs),
        )
        .route("/sessions/{session_id}/runs/{run_id}", get(get_session_run))
        .route(
            "/sessions/{session_id}/runs/{run_id}/live",
            get(session_run_live_page),
        )
        .route(
            "/sessions/{session_id}/runs/{run_id}/artifact",
            get(session_run_artifact_page),
        )
        .route(
            "/sessions/{session_id}/runs/{run_id}/events",
            get(session_run_events_stream),
        )
        .route(
            "/sessions/{session_id}/runs/{run_id}/cancel",
            post(cancel_session_run),
        )
        .route("/runs/{run_id}", get(get_run))
        .route("/runs/{run_id}/live", get(live_page))
        .route("/runs/{run_id}/artifact", get(artifact_page))
        .route("/runs/{run_id}/events", get(events_stream))
        .route("/sessions/{session_id}/cancel", post(cancel_session))
        .route("/runs/{run_id}/cancel", post(cancel_run))
        .route("/templates", get(list_templates))
        .route("/skills", get(list_skills))
        .with_state(state)
}

async fn abort_active_run(state: &AppState) {
    let active_run = {
        let mut active = state.active_run.lock().await;
        active.take()
    };
    if let Some(active_run) = active_run {
        let run_id = active_run.run_id.clone();
        active_run.task.abort();
        StudioRunHandle {
            state: state.clone(),
            run_id,
        }
        .cancel("server shutting down")
        .await;
    }
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    title: String,
    #[serde(flatten)]
    request: RunRequest,
}

#[derive(Debug, Default, Deserialize)]
struct UpdateSessionRequest {
    title: Option<String>,
    instruction: Option<String>,
    input: Option<SessionInput>,
    template_id: Option<String>,
    skill_ids: Option<Vec<String>>,
    provider: Option<String>,
    model: Option<String>,
    options: Option<RunOptions>,
}

#[derive(Debug, Serialize)]
struct SessionsPayload {
    sessions: Vec<StudioSession>,
}

#[derive(Debug, Serialize)]
struct RunsPayload {
    runs: Vec<RunAttempt>,
}

#[derive(Debug, Serialize)]
struct TemplatesPayload {
    templates: Vec<TemplateDescriptor>,
}

#[derive(Debug, Serialize)]
struct SkillsPayload {
    skills: Vec<SkillDescriptor>,
}

#[derive(Debug, Serialize)]
struct ErrorPayload {
    error: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<RunStatus>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    payload: ErrorPayload,
}

impl ApiError {
    fn not_found(kind: &str, id: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            payload: ErrorPayload {
                error: format!("{kind}_not_found"),
                message: format!("{kind} {id} was not found"),
                active_run_id: None,
                active_session_id: None,
                status: None,
            },
        }
    }

    fn active_run_exists(
        active_run_id: String,
        active_session_id: String,
        status: RunStatus,
    ) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            payload: ErrorPayload {
                error: "active_run_exists".to_string(),
                message: "another run is already active".to_string(),
                active_run_id: Some(active_run_id),
                active_session_id: Some(active_session_id),
                status: Some(status),
            },
        }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            payload: ErrorPayload {
                error: "invalid_request".to_string(),
                message: message.into(),
                active_run_id: None,
                active_session_id: None,
                status: None,
            },
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            payload: ErrorPayload {
                error: "internal_error".to_string(),
                message: message.into(),
                active_run_id: None,
                active_session_id: None,
                status: None,
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.payload)).into_response()
    }
}

async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateSessionRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session_id = Uuid::new_v4().to_string();
    let timestamp = now_string();
    let session = StudioSession::new(
        session_id.clone(),
        payload.title,
        payload.request,
        timestamp,
    );
    validate_session(&state, &session)?;

    state.sessions.write().await.insert(
        session_id,
        SessionRecord {
            session: session.clone(),
            run_ids: Vec::new(),
        },
    );
    persist_studio_state_or_api_error(&state).await?;

    Ok((StatusCode::CREATED, Json(session)))
}

async fn list_sessions(State(state): State<AppState>) -> Json<SessionsPayload> {
    let sessions = state
        .sessions
        .read()
        .await
        .values()
        .map(|record| record.session.clone())
        .collect::<Vec<_>>();
    Json(SessionsPayload { sessions })
}

async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> std::result::Result<Json<StudioSession>, ApiError> {
    let sessions = state.sessions.read().await;
    let record = sessions
        .get(&session_id)
        .ok_or_else(|| ApiError::not_found("session", &session_id))?;
    Ok(Json(record.session.clone()))
}

async fn update_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(patch): Json<UpdateSessionRequest>,
) -> std::result::Result<Json<StudioSession>, ApiError> {
    let mut sessions = state.sessions.write().await;
    let record = sessions
        .get_mut(&session_id)
        .ok_or_else(|| ApiError::not_found("session", &session_id))?;
    let mut updated = record.session.clone();

    if let Some(title) = patch.title {
        updated.title = title;
    }
    if let Some(instruction) = patch.instruction {
        updated.request.instruction = instruction;
    }
    if let Some(input) = patch.input {
        updated.request.input = input;
    }
    if let Some(template_id) = patch.template_id {
        updated.request.template_id = template_id;
    }
    if let Some(skill_ids) = patch.skill_ids {
        updated.request.skill_ids = skill_ids;
    }
    if let Some(provider) = patch.provider {
        updated.request.provider = provider;
    }
    if let Some(model) = patch.model {
        updated.request.model = model;
    }
    if let Some(options) = patch.options {
        updated.request.options = options;
    }
    updated.updated_at = now_string();
    validate_session(&state, &updated)?;
    record.session = updated;
    let updated_session = record.session.clone();
    drop(sessions);
    persist_studio_state_or_api_error(&state).await?;

    Ok(Json(updated_session))
}

async fn create_run(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session_snapshot = {
        let sessions = state.sessions.read().await;
        let record = sessions
            .get(&session_id)
            .ok_or_else(|| ApiError::not_found("session", &session_id))?;
        record.session.clone()
    };

    let mut active_guard = state.active_run.lock().await;
    if let Some(active) = active_guard.as_ref() {
        let status = {
            let runs = state.runs.read().await;
            if let Some(shared) = runs.get(&active.run_id) {
                shared.read().await.run.status
            } else {
                RunStatus::Running
            }
        };
        return Err(ApiError::active_run_exists(
            active.run_id.clone(),
            active.session_id.clone(),
            status,
        ));
    }

    let (run, handle) = create_run_entry(&state, &session_id, &session_snapshot).await?;
    let context = RunExecutionContext {
        session: session_snapshot,
        run: run.clone(),
        handle: handle.clone(),
    };
    let executor = Arc::clone(&state.executor);
    let task = tokio::spawn(async move {
        let result = AssertUnwindSafe((executor)(context.clone()))
            .catch_unwind()
            .await;

        match result {
            Ok(Ok(())) => {
                if !handle
                    .current_status()
                    .await
                    .unwrap_or(RunStatus::Completed)
                    .is_terminal()
                {
                    handle.complete(None).await;
                }
            }
            Ok(Err(error)) => handle.fail(format!("{error:#}")).await,
            Err(_) => handle.fail("run executor panicked").await,
        }
    });
    *active_guard = Some(ActiveRun {
        run_id: run.id.clone(),
        session_id,
        task,
    });
    drop(active_guard);

    Ok((StatusCode::CREATED, Json(run)))
}

async fn list_runs(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> std::result::Result<Json<RunsPayload>, ApiError> {
    let run_ids = {
        let sessions = state.sessions.read().await;
        let record = sessions
            .get(&session_id)
            .ok_or_else(|| ApiError::not_found("session", &session_id))?;
        record.run_ids.clone()
    };
    let runs = collect_runs(&state, &run_ids).await;
    Ok(Json(RunsPayload { runs }))
}

async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> std::result::Result<Json<RunAttempt>, ApiError> {
    let run = get_run_handle(&state, &run_id).await?;
    let run = run
        .current_run()
        .await
        .ok_or_else(|| ApiError::not_found("run", &run_id))?;
    Ok(Json(run))
}

async fn studio_shell() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_SECURITY_POLICY, studio_shell_csp_header())
        .body(Body::from(STUDIO_INDEX_HTML))
        .unwrap()
}

async fn studio_stylesheet() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
        .body(Body::from(STUDIO_CSS))
        .unwrap()
}

async fn studio_javascript() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .body(Body::from(STUDIO_JS))
        .unwrap()
}

async fn get_session_run(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
) -> std::result::Result<Json<RunAttempt>, ApiError> {
    ensure_run_belongs_to_session(&state, &session_id, &run_id).await?;
    get_run(State(state), Path(run_id)).await
}

async fn live_page(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> std::result::Result<Html<String>, ApiError> {
    let _ = get_run_handle(&state, &run_id).await?;
    Ok(Html(render_live_page(&run_id)))
}

async fn session_run_live_page(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
) -> std::result::Result<Html<String>, ApiError> {
    ensure_run_belongs_to_session(&state, &session_id, &run_id).await?;
    let _ = get_run_handle(&state, &run_id).await?;
    Ok(Html(render_session_run_live_page(&session_id, &run_id)))
}

async fn artifact_page(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> std::result::Result<Response, ApiError> {
    let handle = get_run_handle(&state, &run_id).await?;
    artifact_response(handle, &run_id).await
}

async fn session_run_artifact_page(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
) -> std::result::Result<Response, ApiError> {
    ensure_run_belongs_to_session(&state, &session_id, &run_id).await?;
    let handle = get_run_handle(&state, &run_id).await?;
    artifact_response(handle, &run_id).await
}

async fn artifact_response(
    handle: StudioRunHandle,
    run_id: &str,
) -> std::result::Result<Response, ApiError> {
    let snapshot = handle
        .snapshot_and_subscribe()
        .await
        .ok_or_else(|| ApiError::not_found("run", run_id))?;
    let mut html_rx = snapshot.html_rx;
    let mut event_rx = snapshot.event_rx;

    let stream = async_stream::stream! {
        let mut emitted_len = snapshot.html.len();
        if !snapshot.html.is_empty() {
            yield Ok::<Bytes, Infallible>(Bytes::from(snapshot.html));
        }

        if snapshot.run.status.is_terminal() {
            return;
        }

        loop {
            tokio::select! {
                recv = html_rx.recv() => match recv {
                    Ok(chunk) => {
                        emitted_len += chunk.len();
                        yield Ok(Bytes::from(chunk));
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let Some(replay) = handle.replay_from(emitted_len).await else {
                            break;
                        };
                        if !replay.suffix.is_empty() {
                            emitted_len += replay.suffix.len();
                            yield Ok(Bytes::from(replay.suffix));
                        }
                        if replay.run.status.is_terminal() {
                            break;
                        }
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                recv = event_rx.recv() => match recv {
                    Ok(run) if run.status.is_terminal() => {
                        let Some(replay) = handle.replay_from(emitted_len).await else {
                            break;
                        };
                        if !replay.suffix.is_empty() {
                            yield Ok(Bytes::from(replay.suffix));
                        }
                        break;
                    }
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let Some(replay) = handle.replay_from(emitted_len).await else {
                            break;
                        };
                        if !replay.suffix.is_empty() {
                            emitted_len += replay.suffix.len();
                            yield Ok(Bytes::from(replay.suffix));
                        }
                        if replay.run.status.is_terminal() {
                            break;
                        }
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache, no-transform")
        .header("x-accel-buffering", "no")
        .header(header::CONTENT_SECURITY_POLICY, artifact_csp_header())
        .body(Body::from_stream(stream))
        .unwrap())
}

async fn events_stream(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> std::result::Result<
    Sse<impl futures::Stream<Item = std::result::Result<Event, Infallible>>>,
    ApiError,
> {
    let handle = get_run_handle(&state, &run_id).await?;
    events_response(handle).await
}

async fn session_run_events_stream(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
) -> std::result::Result<
    Sse<impl futures::Stream<Item = std::result::Result<Event, Infallible>>>,
    ApiError,
> {
    ensure_run_belongs_to_session(&state, &session_id, &run_id).await?;
    let handle = get_run_handle(&state, &run_id).await?;
    events_response(handle).await
}

async fn events_response(
    handle: StudioRunHandle,
) -> std::result::Result<
    Sse<impl futures::Stream<Item = std::result::Result<Event, Infallible>>>,
    ApiError,
> {
    let snapshot = handle
        .snapshot_and_subscribe()
        .await
        .ok_or_else(|| ApiError::not_found("run", "unknown"))?;
    let mut event_rx = snapshot.event_rx;

    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().event("run").data(serialize_run(&snapshot.run)));

        if snapshot.run.status.is_terminal() {
            return;
        }

        loop {
            match event_rx.recv().await {
                Ok(run) => {
                    let terminal = run.status.is_terminal();
                    yield Ok(Event::default().event("run").data(serialize_run(&run)));
                    if terminal {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let Some(replay) = handle.replay_from(0).await else {
                        break;
                    };
                    let terminal = replay.run.status.is_terminal();
                    yield Ok(Event::default().event("run").data(serialize_run(&replay.run)));
                    if terminal {
                        break;
                    }
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(stream))
}

async fn cancel_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> std::result::Result<Json<StudioSession>, ApiError> {
    let active_run_id = {
        let sessions = state.sessions.read().await;
        let record = sessions
            .get(&session_id)
            .ok_or_else(|| ApiError::not_found("session", &session_id))?;
        record.session.active_run_id.clone()
    };

    if let Some(run_id) = active_run_id {
        let _ = cancel_run_by_id(&state, &run_id).await?;
    }

    get_session(State(state), Path(session_id)).await
}

async fn cancel_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> std::result::Result<Json<RunAttempt>, ApiError> {
    cancel_run_by_id(&state, &run_id).await
}

async fn cancel_session_run(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
) -> std::result::Result<Json<RunAttempt>, ApiError> {
    ensure_run_belongs_to_session(&state, &session_id, &run_id).await?;
    cancel_run_by_id(&state, &run_id).await
}

async fn cancel_run_by_id(
    state: &AppState,
    run_id: &str,
) -> std::result::Result<Json<RunAttempt>, ApiError> {
    let handle = get_run_handle(state, run_id).await?;
    {
        let mut active = state.active_run.lock().await;
        if let Some(active_run) = active.take() {
            if active_run.run_id == run_id {
                active_run.task.abort();
            } else {
                *active = Some(active_run);
            }
        }
    }
    handle.cancel("canceled by client").await;
    let run = handle
        .current_run()
        .await
        .ok_or_else(|| ApiError::not_found("run", run_id))?;
    Ok(Json(run))
}

async fn list_templates(State(state): State<AppState>) -> Json<TemplatesPayload> {
    Json(TemplatesPayload {
        templates: state.templates.as_ref().clone(),
    })
}

async fn list_skills(State(state): State<AppState>) -> Json<SkillsPayload> {
    Json(SkillsPayload {
        skills: state.skills.as_ref().clone(),
    })
}

async fn create_run_entry(
    state: &AppState,
    session_id: &str,
    session: &StudioSession,
) -> std::result::Result<(RunAttempt, StudioRunHandle), ApiError> {
    let run_id = Uuid::new_v4().to_string();
    let timestamp = now_string();
    let (attempt, run) = {
        let mut sessions = state.sessions.write().await;
        let record = sessions
            .get_mut(session_id)
            .ok_or_else(|| ApiError::not_found("session", session_id))?;
        let attempt = record.run_ids.len() as u32 + 1;
        let run = RunAttempt {
            id: run_id.clone(),
            session_id: session_id.to_string(),
            attempt,
            status: RunStatus::Running,
            phase: RunPhase::Starting,
            request: session.request.clone(),
            live_url: format!("/sessions/{session_id}/runs/{run_id}/live"),
            artifact_url: format!("/sessions/{session_id}/runs/{run_id}/artifact"),
            events_url: format!("/sessions/{session_id}/runs/{run_id}/events"),
            snapshot_path: None,
            error: None,
            created_at: timestamp.clone(),
            started_at: Some(timestamp.clone()),
            finished_at: None,
        };
        record.run_ids.push(run_id.clone());
        record.session.latest_run_id = Some(run_id.clone());
        record.session.active_run_id = Some(run_id.clone());
        record.session.status = SessionStatus::Active;
        record.session.updated_at = timestamp.clone();
        (attempt, run)
    };

    let (html_tx, _) = broadcast::channel(64);
    let (event_tx, _) = broadcast::channel(64);
    let entry = Arc::new(RwLock::new(RunEntry {
        run: run.clone(),
        html: String::new(),
        html_tx,
        event_tx,
    }));
    entry.read().await.event_tx.send(run.clone()).ok();
    state.runs.write().await.insert(run_id.clone(), entry);
    persist_studio_state_or_api_error(state).await?;

    let handle = StudioRunHandle {
        state: state.clone(),
        run_id,
    };
    debug_assert_eq!(attempt, run.attempt);
    Ok((run, handle))
}

fn validate_session(
    state: &AppState,
    session: &StudioSession,
) -> std::result::Result<(), ApiError> {
    validate_non_empty("title", &session.title)?;
    validate_non_empty("instruction", &session.request.instruction)?;
    validate_non_empty("model", &session.request.model)?;
    validate_non_empty("template_id", &session.request.template_id)?;
    validate_non_empty("provider", &session.request.provider)?;

    if !state
        .templates
        .iter()
        .any(|template| template.id == session.request.template_id)
    {
        return Err(ApiError::invalid_request(format!(
            "unknown template_id: {}",
            session.request.template_id
        )));
    }

    if !matches!(
        session.request.provider.to_ascii_lowercase().as_str(),
        "mock" | "anthropic"
    ) {
        return Err(ApiError::invalid_request(format!(
            "unsupported provider: {}",
            session.request.provider
        )));
    }

    for skill_id in &session.request.skill_ids {
        if !state.skills.iter().any(|skill| skill.name == *skill_id) {
            return Err(ApiError::invalid_request(format!(
                "unknown skill_id: {skill_id}"
            )));
        }
    }

    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> std::result::Result<(), ApiError> {
    if value.trim().is_empty() {
        Err(ApiError::invalid_request(format!(
            "{field} must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn session_status_from_run_status(status: RunStatus) -> SessionStatus {
    match status {
        RunStatus::Running => SessionStatus::Active,
        RunStatus::Completed => SessionStatus::Completed,
        RunStatus::Failed => SessionStatus::Failed,
        RunStatus::Canceled => SessionStatus::Canceled,
    }
}

async fn get_run_handle(
    state: &AppState,
    run_id: &str,
) -> std::result::Result<StudioRunHandle, ApiError> {
    if state.runs.read().await.contains_key(run_id) {
        Ok(StudioRunHandle {
            state: state.clone(),
            run_id: run_id.to_string(),
        })
    } else {
        Err(ApiError::not_found("run", run_id))
    }
}

async fn ensure_run_belongs_to_session(
    state: &AppState,
    session_id: &str,
    run_id: &str,
) -> std::result::Result<(), ApiError> {
    let sessions = state.sessions.read().await;
    let record = sessions
        .get(session_id)
        .ok_or_else(|| ApiError::not_found("session", session_id))?;
    if record.run_ids.iter().any(|id| id == run_id) {
        Ok(())
    } else {
        Err(ApiError::not_found("run", run_id))
    }
}

async fn collect_runs(state: &AppState, run_ids: &[String]) -> Vec<RunAttempt> {
    let runs = state.runs.read().await;
    let mut collected = Vec::with_capacity(run_ids.len());
    for run_id in run_ids {
        if let Some(shared) = runs.get(run_id) {
            collected.push(shared.read().await.run.clone());
        }
    }
    collected
}

fn serialize_run(run: &RunAttempt) -> String {
    serde_json::to_string(run).unwrap_or_else(|_| "{\"status\":\"failed\"}".to_string())
}

fn render_live_page(run_id: &str) -> String {
    render_live_page_with_paths(
        &format!("/runs/{run_id}/artifact"),
        &format!("/runs/{run_id}/events"),
    )
}

fn render_session_run_live_page(session_id: &str, run_id: &str) -> String {
    render_live_page_with_paths(
        &format!("/sessions/{session_id}/runs/{run_id}/artifact"),
        &format!("/sessions/{session_id}/runs/{run_id}/events"),
    )
}

fn render_live_page_with_paths(artifact_path: &str, events_path: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>t2w Studio Preview</title>
  <style>
    body {{ font-family: ui-sans-serif, system-ui, sans-serif; margin: 0; background: #0b1020; color: #e5e7eb; }}
    header {{ display: flex; gap: 16px; align-items: center; padding: 12px 16px; border-bottom: 1px solid #243047; }}
    .pill {{ padding: 4px 10px; border-radius: 999px; background: #162033; color: #93c5fd; font-size: 12px; }}
    #snapshot {{ color: #a7f3d0; font-size: 12px; }}
    iframe {{ width: 100%; height: calc(100vh - 64px); border: 0; background: white; }}
  </style>
</head>
<body>
  <header>
    <strong>t2w studio preview</strong>
    <span id="status" class="pill">starting</span>
    <span id="snapshot"></span>
  </header>
  <iframe id="preview" src="{artifact_path}" sandbox="allow-scripts"></iframe>
  <script>
    const statusEl = document.getElementById('status');
    const snapshotEl = document.getElementById('snapshot');
    const source = new EventSource('{events_path}');
    source.addEventListener('run', (event) => {{
      const payload = JSON.parse(event.data);
      statusEl.textContent = payload.status;
      snapshotEl.textContent = payload.snapshot_path ? `snapshot: ${{payload.snapshot_path}}` : '';
      if (payload.status === 'completed' || payload.status === 'failed' || payload.status === 'canceled') {{
        source.close();
      }}
    }});
  </script>
</body>
</html>"#
    )
}

fn artifact_csp_header() -> &'static str {
    "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; frame-src 'self' data: blob:; child-src 'self' data: blob:; connect-src 'none'; form-action 'none'; frame-ancestors 'self'; base-uri 'none'; object-src 'none'"
}

fn studio_shell_csp_header() -> &'static str {
    "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; connect-src 'self'; font-src 'self' data:; frame-src 'self'; form-action 'self'; frame-ancestors 'self'; base-uri 'none'; object-src 'none'"
}

fn now_string() -> String {
    OffsetDateTime::now_utc()
        .format(&format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second]Z"
        ))
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use agent_core::studio::SkillDescriptor;
    use reqwest::StatusCode;
    use serde_json::json;
    use tokio::sync::Notify;
    use tokio::time::timeout;

    use super::{RunExecutionContext, RunExecutor, StudioServer, StudioServerConfig};

    #[tokio::test]
    async fn studio_shell_route_serves_local_only_app_shell() {
        let server = start_server(immediate_executor()).await;
        let response = reqwest::get(format!("{}/", server.base_url()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.text().await.unwrap();
        assert!(body.contains("id=\"studio-app\""));
        assert!(body.contains("href=\"/studio.css\""));
        assert!(body.contains("src=\"/studio.js\""));
        assert!(body.contains("id=\"run-button\""));
        assert!(body.contains("id=\"download-button\""));
        assert!(body.contains("id=\"preview-frame\""));
        assert!(body.contains("id=\"formula-list\""));
        assert!(body.contains("anthropic (opt-in + key)"));
        assert!(body.contains("/sessions"));
        assert!(body.contains("/templates"));
        assert!(body.contains("/skills"));
        assert!(!body.contains("cdn.tailwindcss.com"));
        assert!(!body.contains("fonts.googleapis.com"));
        assert!(!body.contains("https://"));
        assert!(!body.contains("http://"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn studio_assets_are_served_from_same_origin_routes() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let css = client
            .get(format!("{}/studio.css", server.base_url()))
            .send()
            .await
            .unwrap();
        assert_eq!(css.status(), StatusCode::OK);
        assert_eq!(
            css.headers()
                .get(reqwest::header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "text/css; charset=utf-8"
        );
        let css = css.text().await.unwrap();
        assert!(css.contains("#studio-app"));
        assert!(!css.contains("@import url("));
        assert!(!css.contains("https://"));
        assert!(!css.contains("http://"));

        let js = client
            .get(format!("{}/studio.js", server.base_url()))
            .send()
            .await
            .unwrap();
        assert_eq!(js.status(), StatusCode::OK);
        assert_eq!(
            js.headers()
                .get(reqwest::header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "application/javascript; charset=utf-8"
        );
        let js = js.text().await.unwrap();
        assert!(js.contains("Promise.all"));
        assert!(js.contains("/templates"));
        assert!(js.contains("/skills"));
        assert!(js.contains("/sessions"));
        assert!(js.contains("renderFormulaList"));
        assert!(js.contains("artifactStreamComplete"));
        assert!(js.contains("state.artifactStreamComplete = true"));
        assert!(js.contains("state.artifactStreamComplete"));
        assert!(js.contains("elements.previewFrame.srcdoc = \"\""));
        assert!(js.contains("elements.liveMiniFrame.srcdoc = \"\""));
        assert!(!js.contains("https://"));
        assert!(!js.contains("http://"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn sessions_can_be_created_and_listed() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let created = client
            .post(format!("{}/sessions", server.base_url()))
            .json(&sample_session_payload())
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);

        let sessions = client
            .get(format!("{}/sessions", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        assert_eq!(sessions["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(
            sessions["sessions"][0]["title"].as_str(),
            Some("Production Cluster Logs")
        );
        assert_eq!(sessions["sessions"][0]["status"].as_str(), Some("draft"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn sessions_and_completed_runs_are_restored_from_storage() {
        let temp = tempfile::tempdir().unwrap();
        let storage_path = temp.path().join("studio-state.json");
        let client = reqwest::Client::new();

        let server = start_server_with_storage(immediate_executor(), storage_path.clone()).await;
        let session = create_session(&client, server.base_url()).await;
        let session_id = session["id"].as_str().unwrap().to_string();
        let run = client
            .post(format!("{}/sessions/{session_id}/runs", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let run_id = run["id"].as_str().unwrap().to_string();
        timeout(
            Duration::from_secs(5),
            wait_for_run_status(&client, server.base_url(), &run_id, "completed"),
        )
        .await
        .unwrap();
        server.shutdown().await.unwrap();

        let server = start_server_with_storage(immediate_executor(), storage_path).await;
        let sessions = client
            .get(format!("{}/sessions", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(sessions["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(
            sessions["sessions"][0]["id"].as_str(),
            Some(session_id.as_str())
        );
        assert_eq!(
            sessions["sessions"][0]["latest_run_id"].as_str(),
            Some(run_id.as_str())
        );
        assert_eq!(
            sessions["sessions"][0]["status"].as_str(),
            Some("completed")
        );

        let restored_run = client
            .get(format!("{}/runs/{run_id}", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(restored_run["id"].as_str(), Some(run_id.as_str()));
        assert_eq!(restored_run["status"].as_str(), Some("completed"));
        assert_eq!(restored_run["attempt"].as_u64(), Some(1));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn non_terminal_persisted_runs_are_recovered_as_canceled() {
        let temp = tempfile::tempdir().unwrap();
        let storage_path = temp.path().join("studio-state.json");
        std::fs::write(&storage_path, sample_running_persisted_state()).unwrap();
        let client = reqwest::Client::new();

        let server = start_server_with_storage(immediate_executor(), storage_path).await;
        let sessions = client
            .get(format!("{}/sessions", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(sessions["sessions"][0]["status"].as_str(), Some("canceled"));
        assert!(sessions["sessions"][0]["active_run_id"].is_null());

        let run = client
            .get(format!("{}/runs/run-1", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(run["status"].as_str(), Some("canceled"));
        assert_eq!(run["phase"].as_str(), Some("finished"));
        assert!(run["finished_at"].as_str().is_some());
        assert_eq!(run["error"].as_str(), Some("server restarted during run"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn corrupt_persisted_state_is_preserved_and_does_not_block_startup() {
        let temp = tempfile::tempdir().unwrap();
        let storage_path = temp.path().join("studio-state.json");
        std::fs::write(&storage_path, "{not valid json").unwrap();
        let client = reqwest::Client::new();

        let server = start_server_with_storage(immediate_executor(), storage_path.clone()).await;
        let sessions = client
            .get(format!("{}/sessions", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        assert_eq!(sessions["sessions"].as_array().unwrap().len(), 0);
        assert!(!storage_path.exists());
        assert!(storage_path.with_extension("json.corrupt").exists());

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn sessions_can_be_updated_without_replacing_the_record() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let session_id = session["id"].as_str().unwrap();

        let updated = client
            .patch(format!("{}/sessions/{session_id}", server.base_url()))
            .json(&json!({
                "title": "Retitled Cluster Logs",
                "template_id": "incident-summary"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(updated.status(), StatusCode::OK);
        let updated = updated.json::<serde_json::Value>().await.unwrap();

        assert_eq!(updated["title"].as_str(), Some("Retitled Cluster Logs"));
        assert_eq!(updated["template_id"].as_str(), Some("incident-summary"));
        assert_eq!(updated["id"].as_str(), Some(session_id));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn invalid_session_payloads_are_rejected() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let response = client
            .post(format!("{}/sessions", server.base_url()))
            .json(&json!({
                "title": "Bad Template",
                "instruction": "Build a log artifact",
                "input": {
                    "stdin": "ERROR request failed"
                },
                "template_id": "missing-template",
                "skill_ids": ["log-dashboard"],
                "provider": "mock",
                "model": "mock-model",
                "options": {
                    "persist_snapshot": false
                }
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let payload = response.json::<serde_json::Value>().await.unwrap();
        assert_eq!(payload["error"].as_str(), Some("invalid_request"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn invalid_session_patches_are_rejected_without_mutating() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let session_id = session["id"].as_str().unwrap();

        let response = client
            .patch(format!("{}/sessions/{session_id}", server.base_url()))
            .json(&json!({
                "provider": "not-real"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let current = client
            .get(format!("{}/sessions/{session_id}", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(current["provider"].as_str(), Some("mock"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn runs_expose_live_artifact_and_events_urls() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let run = client
            .post(format!(
                "{}/sessions/{}/runs",
                server.base_url(),
                session["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(run.status(), StatusCode::CREATED);
        let run = run.json::<serde_json::Value>().await.unwrap();

        assert_eq!(run["attempt"].as_u64(), Some(1));
        assert_eq!(run["status"].as_str(), Some("running"));
        assert_eq!(run["phase"].as_str(), Some("starting"));
        assert!(run["artifact_url"].as_str().unwrap().contains("/sessions/"));
        assert!(run["events_url"].as_str().unwrap().contains("/sessions/"));
        assert!(run["live_url"].as_str().unwrap().contains("/sessions/"));

        let artifact = reqwest::get(format!(
            "{}{}",
            server.base_url(),
            run["artifact_url"].as_str().unwrap()
        ))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
        assert!(artifact.contains("Mock Artifact"));

        let events = reqwest::get(format!(
            "{}{}",
            server.base_url(),
            run["events_url"].as_str().unwrap()
        ))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
        assert!(events.contains("completed"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn nested_run_routes_reject_runs_from_other_sessions() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let first_session = create_session(&client, server.base_url()).await;
        let first_run = client
            .post(format!(
                "{}/sessions/{}/runs",
                server.base_url(),
                first_session["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let second_session = create_session(&client, server.base_url()).await;

        let response = client
            .get(format!(
                "{}/sessions/{}/runs/{}",
                server.base_url(),
                second_session["id"].as_str().unwrap(),
                first_run["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn completed_run_replays_artifact_and_events_for_late_subscribers() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let run = client
            .post(format!(
                "{}/sessions/{}/runs",
                server.base_url(),
                session["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let run_id = run["id"].as_str().unwrap();
        timeout(
            Duration::from_secs(5),
            wait_for_run_status(&client, server.base_url(), run_id, "completed"),
        )
        .await
        .unwrap();

        let artifact = reqwest::get(format!(
            "{}{}",
            server.base_url(),
            run["artifact_url"].as_str().unwrap()
        ))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
        assert_eq!(
            artifact,
            "<!DOCTYPE html><html><body>Mock Artifact</body></html>"
        );

        let events = reqwest::get(format!(
            "{}{}",
            server.base_url(),
            run["events_url"].as_str().unwrap()
        ))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
        assert!(events.contains("completed"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn run_handle_recovers_html_when_receiver_lags() {
        let server = start_server(blocking_executor(Arc::new(Notify::new()))).await;
        let client = reqwest::Client::new();
        let session = create_session(&client, server.base_url()).await;
        let run = client
            .post(format!(
                "{}/sessions/{}/runs",
                server.base_url(),
                session["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let handle = super::get_run_handle(&server.state, run["id"].as_str().unwrap())
            .await
            .unwrap();
        let mut html_rx = handle.snapshot_and_subscribe().await.unwrap().html_rx;

        handle.push_html("<html>").await;
        for index in 0..70 {
            handle.push_html(&format!("<p>{index}</p>")).await;
        }

        assert!(matches!(
            html_rx.recv().await.unwrap_err(),
            tokio::sync::broadcast::error::RecvError::Lagged(_)
        ));
        let replay = handle.replay_from(0).await.unwrap();
        assert!(replay.suffix.contains("<html>"));
        assert!(replay.suffix.contains("<p>69</p>"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn cancel_session_is_idempotent_for_draft_sessions() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let response = client
            .post(format!(
                "{}/sessions/{}/cancel",
                server.base_url(),
                session["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let canceled = response.json::<serde_json::Value>().await.unwrap();
        assert_eq!(canceled["status"].as_str(), Some("draft"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn cancel_session_cancels_only_its_active_run() {
        let gate = Arc::new(Notify::new());
        let server = start_server(blocking_executor(Arc::clone(&gate))).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let run = client
            .post(format!(
                "{}/sessions/{}/runs",
                server.base_url(),
                session["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        let canceled = client
            .post(format!(
                "{}/sessions/{}/cancel",
                server.base_url(),
                session["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(canceled["status"].as_str(), Some("canceled"));

        let run = client
            .get(format!(
                "{}/runs/{}",
                server.base_url(),
                run["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(run["status"].as_str(), Some("canceled"));
        assert_eq!(run["phase"].as_str(), Some("finished"));

        gate.notify_waiters();
        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn second_run_is_rejected_while_one_is_active() {
        let gate = Arc::new(Notify::new());
        let server = start_server(blocking_executor(Arc::clone(&gate))).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let session_id = session["id"].as_str().unwrap();

        let first = client
            .post(format!("{}/sessions/{session_id}/runs", server.base_url()))
            .send()
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::CREATED);

        let second = client
            .post(format!("{}/sessions/{session_id}/runs", server.base_url()))
            .send()
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::CONFLICT);

        let payload = second.json::<serde_json::Value>().await.unwrap();
        assert_eq!(payload["error"].as_str(), Some("active_run_exists"));

        gate.notify_waiters();
        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn new_run_is_allowed_after_previous_run_finishes() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let session_id = session["id"].as_str().unwrap();

        let first = client
            .post(format!("{}/sessions/{session_id}/runs", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let first_run_id = first["id"].as_str().unwrap();

        let completed = timeout(
            Duration::from_secs(5),
            wait_for_run_status(&client, server.base_url(), first_run_id, "completed"),
        )
        .await;
        assert!(completed.is_ok());

        let second = client
            .post(format!("{}/sessions/{session_id}/runs", server.base_url()))
            .send()
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::CREATED);
        let second = second.json::<serde_json::Value>().await.unwrap();
        assert_eq!(second["attempt"].as_u64(), Some(2));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn new_run_is_allowed_after_previous_run_fails() {
        let server = start_server(failing_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let session_id = session["id"].as_str().unwrap();

        let first = client
            .post(format!("{}/sessions/{session_id}/runs", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let first_run_id = first["id"].as_str().unwrap();

        let failed = timeout(
            Duration::from_secs(5),
            wait_for_run_status(&client, server.base_url(), first_run_id, "failed"),
        )
        .await;
        assert!(failed.is_ok());

        let second = client
            .post(format!("{}/sessions/{session_id}/runs", server.base_url()))
            .send()
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::CREATED);
        let second = second.json::<serde_json::Value>().await.unwrap();
        assert_eq!(second["attempt"].as_u64(), Some(2));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn editing_a_session_only_affects_future_runs() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let session_id = session["id"].as_str().unwrap();

        let first = client
            .post(format!("{}/sessions/{session_id}/runs", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        client
            .patch(format!("{}/sessions/{session_id}", server.base_url()))
            .json(&json!({
                "template_id": "incident-summary"
            }))
            .send()
            .await
            .unwrap();

        let second = client
            .post(format!("{}/sessions/{session_id}/runs", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        assert_eq!(
            first["request"]["template_id"].as_str(),
            Some("table-explorer")
        );
        assert_eq!(
            second["request"]["template_id"].as_str(),
            Some("incident-summary")
        );

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn templates_and_skills_are_listed() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let templates = client
            .get(format!("{}/templates", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert!(
            templates["templates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|template| template["id"].as_str() == Some("table-explorer"))
        );
        let table_template = templates["templates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|template| template["id"].as_str() == Some("table-explorer"))
            .unwrap();
        let formula_ids = table_template["formula"]
            .as_array()
            .unwrap()
            .iter()
            .map(|stage| stage["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(formula_ids, vec!["prompt", "data", "theme", "run"]);

        let skills = client
            .get(format!("{}/skills", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(skills["skills"].as_array().unwrap().len(), 1);
        assert_eq!(skills["skills"][0]["name"].as_str(), Some("log-dashboard"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn live_preview_page_uses_same_origin_run_routes() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let run = client
            .post(format!(
                "{}/sessions/{}/runs",
                server.base_url(),
                session["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        let live = reqwest::get(format!(
            "{}{}",
            server.base_url(),
            run["live_url"].as_str().unwrap()
        ))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

        let artifact_path = run["artifact_url"].as_str().unwrap();
        let events_path = run["events_url"].as_str().unwrap();
        assert!(live.contains(artifact_path));
        assert!(live.contains(events_path));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn studio_artifact_route_disables_buffering_for_realtime_rendering() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let run = client
            .post(format!(
                "{}/sessions/{}/runs",
                server.base_url(),
                session["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        let response = client
            .get(format!(
                "{}{}",
                server.base_url(),
                run["artifact_url"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap();
        let headers = response.headers();

        assert_eq!(
            headers
                .get(reqwest::header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache, no-transform")
        );
        assert_eq!(
            headers
                .get("x-accel-buffering")
                .and_then(|value| value.to_str().ok()),
            Some("no")
        );

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn cancel_completed_nested_run_is_idempotent() {
        let server = start_server(immediate_executor()).await;
        let client = reqwest::Client::new();

        let session = create_session(&client, server.base_url()).await;
        let session_id = session["id"].as_str().unwrap();
        let run = client
            .post(format!("{}/sessions/{session_id}/runs", server.base_url()))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let run_id = run["id"].as_str().unwrap();
        timeout(
            Duration::from_secs(5),
            wait_for_run_status(&client, server.base_url(), run_id, "completed"),
        )
        .await
        .unwrap();

        let canceled = client
            .post(format!(
                "{}/sessions/{session_id}/runs/{run_id}/cancel",
                server.base_url()
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(canceled.status(), StatusCode::OK);
        let payload = canceled.json::<serde_json::Value>().await.unwrap();
        assert_eq!(payload["status"].as_str(), Some("completed"));

        server.shutdown().await.unwrap();
    }

    async fn start_server(executor: RunExecutor) -> StudioServer {
        StudioServer::start(StudioServerConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            skills: vec![SkillDescriptor {
                name: "log-dashboard".to_string(),
                description: "Turn logs into dashboards".to_string(),
            }],
            executor,
            storage_path: None,
        })
        .await
        .unwrap()
    }

    async fn start_server_with_storage(
        executor: RunExecutor,
        storage_path: std::path::PathBuf,
    ) -> StudioServer {
        StudioServer::start(StudioServerConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            skills: vec![SkillDescriptor {
                name: "log-dashboard".to_string(),
                description: "Turn logs into dashboards".to_string(),
            }],
            executor,
            storage_path: Some(storage_path),
        })
        .await
        .unwrap()
    }

    async fn create_session(client: &reqwest::Client, base_url: &str) -> serde_json::Value {
        client
            .post(format!("{base_url}/sessions"))
            .json(&sample_session_payload())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    fn sample_session_payload() -> serde_json::Value {
        json!({
            "title": "Production Cluster Logs",
            "instruction": "Build a log artifact",
            "input": {
                "stdin": "ERROR request failed"
            },
            "template_id": "table-explorer",
            "skill_ids": ["log-dashboard"],
            "provider": "mock",
            "model": "mock-model",
            "options": {
                "persist_snapshot": false
            }
        })
    }

    fn sample_running_persisted_state() -> String {
        json!({
            "sessions": [{
                "session": {
                    "id": "session-1",
                    "title": "Production Cluster Logs",
                    "status": "active",
                    "instruction": "Build a log artifact",
                    "input": {
                        "stdin": "ERROR request failed"
                    },
                    "template_id": "table-explorer",
                    "skill_ids": ["log-dashboard"],
                    "provider": "mock",
                    "model": "mock-model",
                    "options": {
                        "persist_snapshot": false
                    },
                    "latest_run_id": "run-1",
                    "active_run_id": "run-1",
                    "created_at": "2026-05-24T00:00:00Z",
                    "updated_at": "2026-05-24T00:00:01Z"
                },
                "run_ids": ["run-1"]
            }],
            "runs": [{
                "run": {
                    "id": "run-1",
                    "session_id": "session-1",
                    "attempt": 1,
                    "status": "running",
                    "phase": "streaming",
                    "request": {
                        "instruction": "Build a log artifact",
                        "input": {
                            "stdin": "ERROR request failed"
                        },
                        "template_id": "table-explorer",
                        "skill_ids": ["log-dashboard"],
                        "provider": "mock",
                        "model": "mock-model",
                        "options": {
                            "persist_snapshot": false
                        }
                    },
                    "live_url": "/sessions/session-1/runs/run-1/live",
                    "artifact_url": "/sessions/session-1/runs/run-1/artifact",
                    "events_url": "/sessions/session-1/runs/run-1/events",
                    "snapshot_path": null,
                    "error": null,
                    "created_at": "2026-05-24T00:00:01Z",
                    "started_at": "2026-05-24T00:00:01Z",
                    "finished_at": null
                },
                "html": "<!DOCTYPE html>"
            }]
        })
        .to_string()
    }

    fn immediate_executor() -> RunExecutor {
        Arc::new(|context: RunExecutionContext| {
            Box::pin(async move {
                context
                    .handle
                    .push_html("<!DOCTYPE html><html><body>Mock Artifact</body></html>")
                    .await;
                context.handle.complete(None).await;
                Ok(())
            })
        })
    }

    fn blocking_executor(gate: Arc<Notify>) -> RunExecutor {
        Arc::new(move |context: RunExecutionContext| {
            let gate = Arc::clone(&gate);
            Box::pin(async move {
                context.handle.push_html("<!DOCTYPE html>").await;
                gate.notified().await;
                context
                    .handle
                    .push_html("<html><body>later</body></html>")
                    .await;
                context.handle.complete(None).await;
                Ok(())
            })
        })
    }

    fn failing_executor() -> RunExecutor {
        Arc::new(|context: RunExecutionContext| {
            Box::pin(async move {
                context.handle.push_html("<!DOCTYPE html>").await;
                anyhow::bail!("mock executor failed");
            })
        })
    }

    async fn wait_for_run_status(
        client: &reqwest::Client,
        base_url: &str,
        run_id: &str,
        expected: &str,
    ) {
        loop {
            let run = client
                .get(format!("{base_url}/runs/{run_id}"))
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap();
            if run["status"].as_str() == Some(expected) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}
