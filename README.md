# t2w

`t2w` is a Rust CLI that turns terminal input into a live local web artifact.

The v1 loop is intentionally narrow:

`stdin / instruction -> prompt assembly -> Anthropic SSE -> local HTTP preview -> browser live render -> optional HTML snapshot`

## What v1 does

- Opens a local live preview page immediately.
- Streams HTML chunks from the model into the browser as they arrive.
- Saves a final `.html` snapshot after completion unless `--no-snapshot` is set.
- Supports one real provider (`anthropic`) and one deterministic test provider (`mock`).
- Discovers local `SKILL.md` files and injects matched skills into the prompt.

## Quick Start

1. Install Rust.
2. Build the workspace:

```powershell
cargo build
```

3. Set your Anthropic configuration:

```powershell
$env:T2W_ANTHROPIC_API_KEY = "your-key"
$env:T2W_ANTHROPIC_MODEL = "claude-sonnet-4-5"
```

4. Run the live preview flow:

```powershell
Get-Content .\access.log | cargo run -p agent-cli -- "把这些日志做成错误分析仪表盘"
```

Or use the mock provider for local testing:

```powershell
Get-Content .\access.log | cargo run -p agent-cli -- --provider mock --no-open "build a mock error dashboard"
```

The CLI prints a live preview URL like `http://127.0.0.1:PORT/live/SESSION_ID`.

## CLI

```text
t2w "<instruction>"
  --provider anthropic|mock
  --skill <name>
  --skills-dir <path>
  --config <path>
  --artifacts-dir <path>
  --no-open
  --no-snapshot
```

## Skills

v1 supports local skill discovery and prompt injection only.

- Default search roots:
  - `./skills`
  - `./.t2w/skills`
- Extra roots can be added with `--skills-dir`.
- Explicit activation uses `--skill <name>`.
- Automatic activation order is:
  - explicit name
  - `trigger` match
  - `description` token overlap

Example skill: [examples/skills/log-dashboard/SKILL.md](C:\Users\T\Desktop\t2w\examples\skills\log-dashboard\SKILL.md)

## Output

- Live page: `/live/<session_id>`
- Streaming HTML artifact: `/artifact/<session_id>`
- Session events: `/events/<session_id>`
- Snapshot directory by default: `.t2w/artifacts/`

Snapshots are a persistence side effect, not the primary product.

## Quality Gates

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test -p agent-cli --test live_preview
```

Or use `xtask`:

```powershell
cargo run -p xtask -- fmt
cargo run -p xtask -- clippy
cargo run -p xtask -- test
cargo run -p xtask -- smoke
```
