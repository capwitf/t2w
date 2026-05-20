# Session Log

Date: 2026-05-20

## Current State

v1 is complete for the local Rust CLI + Studio shell scope.

Completed:

- Rust workspace structure exists: `agent-core`, `agent-skills`, `agent-server`, `agent-cli`, `agent-tui`, `xtask`.
- CLI v1 can run the `stdin / instruction -> prompt -> provider stream -> local preview -> optional snapshot` flow.
- Anthropic SSE provider, mock provider, HTML stream sanitizer, local preview server, snapshot writing, and skill discovery/injection are implemented.
- Studio backend exists with sessions, run attempts, run streaming, single active run guard, templates/skills endpoints, cancel endpoints, and same-origin live/artifact/events routes.
- Studio frontend now ships from `GET /` as embedded same-origin assets in `agent-server`, with working Prompt/Data/Theme/Run tabs, local draft behavior, session reload, run launch, SSE status updates, preview iframe, cancel path, and real `Download HTML`.

Verification evidence collected for the v1 finish:

- Rust route and asset coverage:
  - `cargo test -p agent-server studio_shell_route_serves_local_only_app_shell -- --nocapture`
  - `cargo test -p agent-server studio_assets_are_served_from_same_origin_routes -- --nocapture`
- Studio binary integration coverage:
  - `cargo test -p agent-cli --test studio_server -- --nocapture`
- Browser smoke:
  - loaded `GET /` with no console errors
  - verified same-origin page assets and API requests
  - created a mock session, completed a run, rendered `Mock Artifact` in the iframe
  - downloaded a real `.html` artifact file from the Studio shell
  - verified 390px mobile layout after fixing header overlap on the config toggle
- Workspace quality gates passed before this finish plan started:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`

Memory-state limitation:

- Studio sessions and runs are process-memory only.
- Refreshing the browser rehydrates from the current `t2w-studio` process.
- Restarting the Studio server drops all Studio session/run state.

## Post-v1 Notes

Documentation / packaging follow-up completed:

- Rewrote `README.md` as the main English project homepage and added `README_zh.md`.
- Clarified source-run and install flows:
  - source run: `cargo run -p agent-cli --bin t2w -- ...`
  - optional install: `cargo install --path crates/agent-cli --bin t2w --bin t2w-studio --force`
- Updated root `.gitignore` for local OMX and Playwright MCP runtime artifacts:
  - `/.omx/`
  - `/.playwright-mcp/`

Product-positioning clarification from manual testing:

- `t2w` is CLI-first. The primary story is still "enter an instruction in the CLI and get an HTML artifact".
- The Studio frontend is useful as the local control-plane UI:
  - edit session fields
  - inspect run state
  - preview artifacts
  - download final HTML
- The Studio frontend remains the local control-plane shell around Studio workflows.
- Generated artifacts now use a separate default artifact shell based on the Stitch `T2W Industrial Studio` design direction.
- The current `mock` provider still proves the inner artifact pipeline and returns a fixed minimal HTML page (`Mock Artifact`), but the default shell now provides the polished artifact workbench around it.

Default artifact shell follow-up completed:

- Used `$design-md` to normalize the Stitch export into project `DESIGN.md`.
- Added `agent-core::artifact_shell` as the reusable default artifact shell renderer.
- CLI one-shot and Studio runs now wrap provider HTML in the default `T2W Industrial Studio` shell.
- The shell is self-contained: inline CSS/JS, no CDN, no external fonts, no Material Symbols.
- The generated provider HTML is kept separate in the center preview iframe.
- Right utility panel interactions are implemented:
  - Prompt/Data/Theme/Run tabs
  - copy instruction
  - fullscreen preview
  - export/download inner HTML
  - mock `Run/Generate` status transition: `ready -> streaming -> completed`
- CLI live preview and saved snapshots now use the same shell contract.
- Studio artifact route and persisted Studio snapshots now use the same shell contract.

Verification evidence for default artifact shell:

- TDD red/green coverage:
  - `cargo test -p agent-core artifact_shell -- --nocapture`
  - `cargo test -p agent-cli --test live_preview mock_cli_emits_live_url_streams_artifact_and_writes_snapshot -- --nocapture`
  - `cargo test -p agent-cli --test studio_server studio_binary_starts_and_serves_session_run_api -- --nocapture`
- Workspace quality gates:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
- Browser smoke:
  - generated a mock CLI artifact through `cargo run -p agent-cli --bin t2w -- --provider mock --no-open`
  - opened the saved artifact through a temporary local HTTP server
  - verified right-panel Theme/Run tab switching
  - verified `Run/Generate` transitions to `completed`
  - verified browser console errors: `0`

Important live-preview behavior note:

- The one-shot CLI starts a temporary preview server, prints `Live preview: http://127.0.0.1:.../live/...`, streams into it, writes the snapshot, then shuts the server down when the run completes.
- Because `mock` finishes very quickly, opening the printed live URL after the process exits often results in `ERR_CONNECTION_REFUSED`.
- For screenshot capture today:
  - use the saved `.html` file under `.t2w/artifacts/` for final artifact screenshots
  - or slow mock streaming with `T2W_MOCK_CHUNK_DELAY_MS` and avoid `--no-open` when capturing an in-flight preview

Docs/media guidance captured from this session:

- The existing `screen.png` is treated as an outdated mockup and should not be used as the primary README visual.
- Better README media order:
  - CLI command screenshot
  - final artifact screenshot from `.t2w/artifacts/`
  - optional Studio shell screenshot as secondary documentation

## v1.x Backlog

- Real TUI layer beyond the current Studio shell.
- Advanced `SKILL.md` workflows beyond local discovery/injection:
  - install/market flow
  - tool sandboxing and enforcement
  - richer dynamic context injection
- Additional providers beyond `anthropic` and `mock`.
- Optional Studio persistence beyond in-memory process state.
- Further UI polish that is explicitly out of v1:
  - richer session history tools
  - inspector variants
  - production screenshots/GIFs and docs split
