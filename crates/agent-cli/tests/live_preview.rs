use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

#[tokio::test]
async fn mock_cli_emits_live_url_streams_artifact_and_writes_snapshot() {
    let temp = tempdir().unwrap();
    let mut child = spawn_cli(temp.path(), false).await;

    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let first_line = timeout(Duration::from_secs(5), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert!(first_line.starts_with("Live preview: "));
    let live_url = first_line.trim_start_matches("Live preview: ").to_string();
    let live_body = reqwest::get(&live_url).await.unwrap().text().await.unwrap();
    assert!(live_body.contains("/artifact/"));

    let artifact_url = live_url.replace("/live/", "/artifact/");
    let artifact_body = reqwest::get(&artifact_url)
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(artifact_body.contains("Mock Artifact"));
    assert!(artifact_body.contains("Streaming preview is active."));
    assert!(artifact_body.contains("data-t2w-artifact-shell=\"default\""));
    assert!(artifact_body.contains("User Instruction"));
    assert!(artifact_body.contains("build a mock error dashboard"));
    assert!(artifact_body.contains("GET /health 500"));

    let status = timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success());

    let artifacts_dir = temp.path().join(".t2w").join("artifacts");
    let snapshots = collect_html_files(&artifacts_dir);
    assert_eq!(snapshots.len(), 1);
    let snapshot_html = fs::read_to_string(&snapshots[0]).unwrap();
    assert!(snapshot_html.starts_with("<!DOCTYPE html>"));
    assert!(snapshot_html.contains("data-t2w-artifact-shell=\"default\""));
    assert!(snapshot_html.contains("Mock Artifact"));
    assert!(snapshot_html.contains("build a mock error dashboard"));
    assert!(snapshot_html.contains("GET /ready 503"));
    assert!(!snapshot_html.contains("```"));
    assert!(!snapshot_html.contains("Here is your artifact"));
}

#[tokio::test]
async fn mock_cli_can_skip_snapshot_persistence() {
    let temp = tempdir().unwrap();
    let mut child = spawn_cli(temp.path(), true).await;

    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let first_line = timeout(Duration::from_secs(5), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(first_line.starts_with("Live preview: "));

    let status = timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success());

    let artifacts_dir = temp.path().join(".t2w").join("artifacts");
    let snapshots = collect_html_files(&artifacts_dir);
    assert!(snapshots.is_empty());
}

async fn spawn_cli(cwd: &std::path::Path, no_snapshot: bool) -> tokio::process::Child {
    let bin = std::env::var("CARGO_BIN_EXE_t2w").unwrap();
    let mut command = Command::new(bin);
    command
        .current_dir(cwd)
        .arg("--provider")
        .arg("mock")
        .arg("--no-open");
    if no_snapshot {
        command.arg("--no-snapshot");
    }
    command
        .arg("build a mock error dashboard")
        .env("T2W_MOCK_CHUNK_DELAY_MS", "150")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(b"GET /health 500\nGET /ready 503\n")
        .await
        .unwrap();
    drop(stdin);
    child
}

fn collect_html_files(dir: &PathBuf) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }

    let mut files = fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("html"))
        .collect::<Vec<_>>();
    files.sort();
    files
}
