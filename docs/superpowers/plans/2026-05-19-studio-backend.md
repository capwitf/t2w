# Studio Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a local HTTP studio backend with editable sessions, append-only run attempts, same-origin preview routes, and a single active run guard without breaking the current one-shot `t2w "<instruction>"` flow.

**Architecture:** Keep the existing one-shot preview server intact. Add a new `StudioServer` surface in `agent-server` that owns session/run metadata and preview routes for studio mode, while `agent-cli` provides a `t2w-studio` binary that injects the actual run executor using the same prompt/provider pipeline we already trust.

**Tech Stack:** Rust, Axum, Tokio, Serde, Reqwest, existing `agent-core`/`agent-server`/`agent-skills` crates.

---

### Task 1: Add shared studio domain models

**Files:**
- Create: `C:/Users/T/Desktop/t2w/crates/agent-core/src/studio.rs`
- Modify: `C:/Users/T/Desktop/t2w/crates/agent-core/src/lib.rs`
- Test: `C:/Users/T/Desktop/t2w/crates/agent-core/src/studio.rs`

- [ ] **Step 1: Write the failing tests**

Add unit tests for:
- session creation metadata
- run attempt numbering
- template catalog exposure
- terminal status helpers

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent-core studio`
Expected: FAIL because `studio.rs` and exported types do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- `StudioSession`
- `RunAttempt`
- `RunStatus`
- `RunRequest`
- `SessionInput`
- `RunOptions`
- `TemplateDescriptor`
- `default_templates()`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p agent-core studio`
Expected: PASS

### Task 2: Add server-side studio API and preview streaming

**Files:**
- Modify: `C:/Users/T/Desktop/t2w/crates/agent-server/src/lib.rs`
- Create: `C:/Users/T/Desktop/t2w/crates/agent-server/src/studio.rs`
- Test: `C:/Users/T/Desktop/t2w/crates/agent-server/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Add integration-style async tests for:
- `POST /sessions` creates a session
- `GET /sessions` returns stored sessions
- `PATCH /sessions/{id}` updates editable session fields without mutating prior runs
- `POST /sessions/{id}/runs` creates attempt 1 and exposes artifact/events/live URLs
- creating a second run while one is active returns `409`
- active-run guard releases after completion and after executor failure
- `GET /runs/{id}/artifact` and `GET /runs/{id}/events` stream executor output
- `GET /runs/{id}/live` embeds same-origin artifact and event URLs
- `GET /templates` returns the template catalog
- `GET /skills` returns the startup skill summary list

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent-server studio`
Expected: FAIL because the routes and studio server types do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- `StudioServer`
- in-memory session/run store
- single-active-run gate
- run-scoped preview/event channels
- injected executor hook
- JSON API routes
- explicit error payloads for conflict/not-found/update validation

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p agent-server studio`
Expected: PASS

### Task 3: Add the `t2w-studio` launcher and executor bridge

**Files:**
- Modify: `C:/Users/T/Desktop/t2w/crates/agent-cli/src/lib.rs`
- Create: `C:/Users/T/Desktop/t2w/crates/agent-cli/src/bin/t2w-studio.rs`
- Modify: `C:/Users/T/Desktop/t2w/crates/agent-cli/Cargo.toml`
- Test: `C:/Users/T/Desktop/t2w/crates/agent-cli/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Add focused tests for:
- building a run request into prompt/provider inputs
- saving snapshots for studio runs
- static skill discovery payload shaping
- the existing one-shot `t2w "<instruction>" --provider mock --no-open` path still works unchanged

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent-cli studio`
Expected: FAIL because the studio executor helpers and binary entrypoint do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- executor helper that consumes a `RunRequest` + run handle
- skill discovery summary helper for studio startup
- `t2w-studio` binary that starts the server and prints its base URL

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p agent-cli studio`
Expected: PASS

### Task 4: Verify whole-workspace behavior

**Files:**
- Modify: `C:/Users/T/Desktop/t2w/crates/agent-cli/src/lib.rs` (only if verification reveals gaps)
- Modify: `C:/Users/T/Desktop/t2w/crates/agent-server/src/lib.rs` (only if verification reveals gaps)

- [ ] **Step 1: Run formatting**

Run: `cargo fmt --all --check`
Expected: PASS

- [ ] **Step 2: Run lint**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 3: Run tests**

Run: `cargo test --workspace`
Expected: PASS

- [ ] **Step 4: Review the final diff**

Check changed files stay scoped to studio domain models, server API, and the new launcher path.
