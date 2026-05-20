use std::process::Stdio;
use std::time::Duration;

use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

#[tokio::test]
async fn studio_binary_starts_and_serves_session_run_api() {
    let temp = tempdir().unwrap();
    let bin = std::env::var("CARGO_BIN_EXE_t2w-studio").unwrap();
    let mut child = Command::new(bin)
        .current_dir(temp.path())
        .arg("--port")
        .arg("0")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let first_line = timeout(Duration::from_secs(5), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(first_line.starts_with("Studio server: "));
    let base_url = first_line
        .trim_start_matches("Studio server: ")
        .trim_end_matches('/')
        .to_string();

    let client = reqwest::Client::new();
    let shell = client.get(format!("{base_url}/")).send().await.unwrap();
    assert!(shell.status().is_success());
    let shell = shell.text().await.unwrap();
    assert!(shell.contains("id=\"studio-app\""));
    assert!(shell.contains("href=\"/studio.css\""));
    assert!(shell.contains("src=\"/studio.js\""));
    assert!(shell.contains("id=\"run-button\""));
    assert!(shell.contains("id=\"download-button\""));

    let session = client
        .post(format!("{base_url}/sessions"))
        .json(&serde_json::json!({
            "title": "Mock Artifact",
            "instruction": "build a mock artifact",
            "input": { "stdin": "GET /health 500" },
            "template_id": "table-explorer",
            "skill_ids": [],
            "provider": "mock",
            "model": "mock-model",
            "options": { "persist_snapshot": false }
        }))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let run = client
        .post(format!(
            "{base_url}/sessions/{}/runs",
            session["id"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let artifact = reqwest::get(format!(
        "{base_url}{}",
        run["artifact_url"].as_str().unwrap()
    ))
    .await
    .unwrap()
    .text()
    .await
    .unwrap();
    assert!(artifact.contains("Mock Artifact"));
    assert!(artifact.contains("data-t2w-artifact-shell=\"default\""));
    assert!(artifact.contains("User Instruction"));
    assert!(artifact.contains("build a mock artifact"));
    assert!(artifact.contains("GET /health 500"));

    let snapshot_session = client
        .post(format!("{base_url}/sessions"))
        .json(&serde_json::json!({
            "title": "Snapshot Artifact",
            "instruction": "build a persisted mock artifact",
            "input": { "stdin": "GET /ready 503" },
            "template_id": "table-explorer",
            "skill_ids": [],
            "provider": "mock",
            "model": "mock-model",
            "options": { "persist_snapshot": true }
        }))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let snapshot_run = client
        .post(format!(
            "{base_url}/sessions/{}/runs",
            snapshot_session["id"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let snapshot_run = timeout(
        Duration::from_secs(5),
        wait_for_completed_run(&client, &base_url, snapshot_run["id"].as_str().unwrap()),
    )
    .await
    .unwrap();

    let snapshot_path = snapshot_run["snapshot_path"].as_str().unwrap();
    assert!(std::path::Path::new(snapshot_path).exists());
    let snapshot_html = std::fs::read_to_string(snapshot_path).unwrap();
    assert!(snapshot_html.contains("data-t2w-artifact-shell=\"default\""));
    assert!(snapshot_html.contains("build a persisted mock artifact"));
    assert!(snapshot_html.contains("GET /ready 503"));

    child.kill().await.unwrap();
}

async fn wait_for_completed_run(
    client: &reqwest::Client,
    base_url: &str,
    run_id: &str,
) -> serde_json::Value {
    loop {
        let run = client
            .get(format!("{base_url}/runs/{run_id}"))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        if run["status"].as_str() == Some("completed") {
            return run;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
