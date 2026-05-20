use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use agent_core::session::{ArtifactSession, SessionEvent, SessionState};
use anyhow::{Result, anyhow};
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event, Sse};
use axum::response::{Html, Response};
use axum::routing::get;
use bytes::Bytes;
use tokio::net::TcpListener;
use tokio::sync::{RwLock, broadcast, oneshot};
use tokio::task::JoinHandle;

pub mod studio;

#[derive(Clone)]
struct AppState {
    sessions: Arc<RwLock<HashMap<String, SharedSession>>>,
}

type SharedSession = Arc<RwLock<SessionEntry>>;

struct SessionEntry {
    session: ArtifactSession,
    html_tx: broadcast::Sender<String>,
    event_tx: broadcast::Sender<SessionEvent>,
}

#[derive(Clone)]
pub struct SessionHandle {
    inner: SharedSession,
}

impl SessionHandle {
    pub async fn push_html(&self, chunk: &str) {
        let mut entry = self.inner.write().await;
        entry.session.push_chunk(chunk);
        let _ = entry.html_tx.send(chunk.to_string());
        let _ = entry.event_tx.send(entry.session.event());
    }

    pub async fn complete(&self, snapshot_path: Option<PathBuf>) {
        let mut entry = self.inner.write().await;
        entry.session.complete(snapshot_path);
        let _ = entry.event_tx.send(entry.session.event());
    }

    pub async fn fail(&self, message: impl Into<String>) {
        let mut entry = self.inner.write().await;
        entry.session.fail(message.into());
        let _ = entry.event_tx.send(entry.session.event());
    }

    async fn snapshot_and_subscribe(&self) -> SessionStreamSnapshot {
        let entry = self.inner.read().await;
        SessionStreamSnapshot {
            html: entry.session.full_html().to_string(),
            event: entry.session.event(),
            html_rx: entry.html_tx.subscribe(),
            event_rx: entry.event_tx.subscribe(),
        }
    }

    async fn replay_from(&self, offset: usize) -> SessionReplay {
        let entry = self.inner.read().await;
        let html = entry.session.full_html();
        SessionReplay {
            suffix: html.get(offset..).unwrap_or("").to_string(),
            event: entry.session.event(),
        }
    }
}

struct SessionStreamSnapshot {
    html: String,
    event: SessionEvent,
    html_rx: broadcast::Receiver<String>,
    event_rx: broadcast::Receiver<SessionEvent>,
}

struct SessionReplay {
    suffix: String,
    event: SessionEvent,
}

pub struct PreviewServer {
    base_url: String,
    state: AppState,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: JoinHandle<Result<()>>,
}

impl PreviewServer {
    pub async fn start() -> Result<Self> {
        let state = AppState {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let app = Router::new()
            .route("/live/{session_id}", get(live_page))
            .route("/artifact/{session_id}", get(artifact_page))
            .route("/events/{session_id}", get(events_stream))
            .with_state(state.clone());
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server_task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .map_err(Into::into)
        });

        Ok(Self {
            base_url: format!("http://{address}"),
            state,
            shutdown_tx: Some(shutdown_tx),
            server_task,
        })
    }

    pub async fn create_session(&self, session_id: String) -> Result<SessionHandle> {
        let (html_tx, _) = broadcast::channel(64);
        let (event_tx, _) = broadcast::channel(64);
        let session = ArtifactSession::new(session_id.clone());
        let initial_event = session.event();
        let entry = Arc::new(RwLock::new(SessionEntry {
            session,
            html_tx,
            event_tx,
        }));
        entry.read().await.event_tx.send(initial_event).ok();

        let mut sessions = self.state.sessions.write().await;
        if sessions.contains_key(&session_id) {
            return Err(anyhow!("session {session_id} already exists"));
        }
        sessions.insert(session_id, entry.clone());

        Ok(SessionHandle { inner: entry })
    }

    pub fn live_url(&self, session_id: &str) -> String {
        format!("{}/live/{session_id}", self.base_url)
    }

    pub fn artifact_url(&self, session_id: &str) -> String {
        format!("{}/artifact/{session_id}", self.base_url)
    }

    pub fn events_url(&self, session_id: &str) -> String {
        format!("{}/events/{session_id}", self.base_url)
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.server_task.await??;
        Ok(())
    }
}

async fn live_page(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let _ = get_session(&state, &session_id).await?;
    Ok(Html(render_live_page(&session_id)))
}

async fn artifact_page(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Response, StatusCode> {
    let session = get_session(&state, &session_id).await?;
    let snapshot = session.snapshot_and_subscribe().await;
    let mut html_rx = snapshot.html_rx;
    let mut event_rx = snapshot.event_rx;

    let stream = async_stream::stream! {
        let mut emitted_len = snapshot.html.len();
        if !snapshot.html.is_empty() {
            yield Ok::<Bytes, Infallible>(Bytes::from(snapshot.html));
        }

        if matches!(snapshot.event.state, SessionState::Completed | SessionState::Failed) {
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
                        let replay = session.replay_from(emitted_len).await;
                        if !replay.suffix.is_empty() {
                            emitted_len += replay.suffix.len();
                            yield Ok(Bytes::from(replay.suffix));
                        }
                        if matches!(replay.event.state, SessionState::Completed | SessionState::Failed) {
                            break;
                        }
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                recv = event_rx.recv() => match recv {
                    Ok(event) if matches!(event.state, SessionState::Completed | SessionState::Failed) => {
                        let replay = session.replay_from(emitted_len).await;
                        if !replay.suffix.is_empty() {
                            yield Ok(Bytes::from(replay.suffix));
                        }
                        break;
                    }
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let replay = session.replay_from(emitted_len).await;
                        if !replay.suffix.is_empty() {
                            emitted_len += replay.suffix.len();
                            yield Ok(Bytes::from(replay.suffix));
                        }
                        if matches!(replay.event.state, SessionState::Completed | SessionState::Failed) {
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
        .header(header::CONTENT_SECURITY_POLICY, artifact_csp_header())
        .body(Body::from_stream(stream))
        .unwrap())
}

async fn events_stream(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let session = get_session(&state, &session_id).await?;
    let snapshot = session.snapshot_and_subscribe().await;
    let mut event_rx = snapshot.event_rx;

    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().event("session").data(serialize_event(&snapshot.event)));

        if matches!(snapshot.event.state, SessionState::Completed | SessionState::Failed) {
            return;
        }

        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    let terminal = matches!(event.state, SessionState::Completed | SessionState::Failed);
                    yield Ok(Event::default().event("session").data(serialize_event(&event)));
                    if terminal {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let replay = session.replay_from(0).await;
                    yield Ok(Event::default().event("session").data(serialize_event(&replay.event)));
                    if matches!(replay.event.state, SessionState::Completed | SessionState::Failed) {
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

async fn get_session(state: &AppState, session_id: &str) -> Result<SessionHandle, StatusCode> {
    let sessions = state.sessions.read().await;
    let session = sessions
        .get(session_id)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(SessionHandle { inner: session })
}

fn serialize_event(event: &SessionEvent) -> String {
    serde_json::to_string(event).unwrap_or_else(|_| "{\"state\":\"failed\"}".to_string())
}

fn render_live_page(session_id: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>t2w Live Preview</title>
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
    <strong>t2w live preview</strong>
    <span id="status" class="pill">pending</span>
    <span id="snapshot"></span>
  </header>
  <iframe id="preview" src="/artifact/{session_id}" sandbox="allow-scripts"></iframe>
  <script>
    const statusEl = document.getElementById('status');
    const snapshotEl = document.getElementById('snapshot');
    const source = new EventSource('/events/{session_id}');
    source.addEventListener('session', (event) => {{
      const payload = JSON.parse(event.data);
      statusEl.textContent = payload.state;
      snapshotEl.textContent = payload.snapshot_path ? `snapshot: ${{payload.snapshot_path}}` : '';
      if (payload.state === 'completed' || payload.state === 'failed') {{
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use tokio::time::timeout;

    use crate::PreviewServer;

    #[tokio::test]
    async fn live_page_contains_iframe_and_event_source() {
        let server = PreviewServer::start().await.unwrap();
        server.create_session("demo".to_string()).await.unwrap();

        let body = reqwest::get(server.live_url("demo"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        assert!(body.contains("iframe"));
        assert!(body.contains("/artifact/demo"));
        assert!(body.contains("/events/demo"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn artifact_route_streams_existing_and_future_html() {
        let server = PreviewServer::start().await.unwrap();
        let session = server
            .create_session("streaming".to_string())
            .await
            .unwrap();
        session.push_html("<!DOCTYPE html>").await;

        let artifact_url = server.artifact_url("streaming");
        let fetch = tokio::spawn(async move {
            reqwest::get(artifact_url)
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        session.push_html("<html><body>hello</body></html>").await;
        session
            .complete(Some(PathBuf::from(".t2w/artifacts/final.html")))
            .await;

        let body = timeout(Duration::from_secs(5), fetch)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(body, "<!DOCTYPE html><html><body>hello</body></html>");

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn artifact_route_replays_completed_html_for_late_connections() {
        let server = PreviewServer::start().await.unwrap();
        let session = server.create_session("late".to_string()).await.unwrap();
        session.push_html("<!DOCTYPE html><html></html>").await;
        session.complete(None).await;

        let body = reqwest::get(server.artifact_url("late"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        assert_eq!(body, "<!DOCTYPE html><html></html>");

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn artifact_route_recovers_html_when_receiver_lags() {
        let (html_tx, _) = tokio::sync::broadcast::channel(1);
        let (event_tx, _) = tokio::sync::broadcast::channel(1);
        let mut artifact = agent_core::session::ArtifactSession::new("lag".to_string());
        artifact.push_chunk("<!DOCTYPE html>");
        let entry = std::sync::Arc::new(tokio::sync::RwLock::new(super::SessionEntry {
            session: artifact,
            html_tx,
            event_tx,
        }));
        let handle = super::SessionHandle { inner: entry };
        let mut html_rx = handle.snapshot_and_subscribe().await.html_rx;

        handle.push_html("<html>").await;
        handle.push_html("<body>").await;
        assert!(matches!(
            html_rx.recv().await.unwrap_err(),
            tokio::sync::broadcast::error::RecvError::Lagged(_)
        ));

        let replay = handle.replay_from("<!DOCTYPE html>".len()).await;
        assert_eq!(replay.suffix, "<html><body>");
    }

    #[tokio::test]
    async fn events_route_reports_completed_state_and_snapshot() {
        let server = PreviewServer::start().await.unwrap();
        let session = server.create_session("events".to_string()).await.unwrap();
        session.push_html("<!DOCTYPE html>").await;
        session
            .complete(Some(PathBuf::from(".t2w/artifacts/events.html")))
            .await;

        let body = reqwest::get(server.events_url("events"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        assert!(body.contains("completed"));
        assert!(body.contains(".t2w/artifacts/events.html"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn artifact_route_sets_restrictive_csp_header() {
        let server = PreviewServer::start().await.unwrap();
        let session = server.create_session("csp".to_string()).await.unwrap();
        session.push_html("<!DOCTYPE html><html></html>").await;
        session.complete(None).await;

        let response = reqwest::get(server.artifact_url("csp")).await.unwrap();
        let csp = response
            .headers()
            .get(reqwest::header::CONTENT_SECURITY_POLICY)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        assert!(csp.contains("connect-src 'none'"));
        assert!(csp.contains("script-src 'unsafe-inline'"));
        assert!(csp.contains("frame-ancestors 'self'"));
        assert!(!csp.contains("frame-ancestors 'none'"));

        server.shutdown().await.unwrap();
    }
}
