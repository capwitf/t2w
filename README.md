[English](README.md) | [中文](README_zh.md)

# t2w

`t2w` is a Rust-first local artifact engine that turns terminal input into live HTML. It gives you a one-shot CLI for fast generation and a local Studio shell for inspecting prompts, data, run state, and final artifacts.

## Preview

![t2w artifact shell](docs/assets/t2w-artifact-shell.png)

The default artifact shell wraps generated HTML with a local workbench: prompt context, data view, theme controls, run status, fullscreen preview, and HTML export.

![t2w Studio shell](docs/assets/t2w-studio-shell.png)

Studio runs as a local same-origin web app served by `t2w-studio`; no Node build, CDN, external fonts, or hosted service is required.

## Highlights

- Stream `stdin + instruction` into a live browser preview.
- Save the final result as a self-contained `.html` artifact.
- Run a local Studio UI with sessions, templates, skills, active-run status, cancel, preview, and download.
- Use a deterministic `mock` provider for smoke tests and an `anthropic` provider for real generation.
- Discover local `SKILL.md` files and inject relevant skill instructions into prompts.
- Ship embedded Studio assets from the Rust server, so releases are just portable binaries.

## Install

The v1.x distribution is intentionally lightweight: download an archive, unpack it, and run the binaries. Native installers such as `.msi`, `.pkg`, `.deb`, and AppImage are planned after the CLI and Studio contracts stabilize.

| Platform | Package |
| --- | --- |
| Windows x64 | [t2w-windows-x86_64.zip](https://github.com/capwitf/t2w/releases/latest/download/t2w-windows-x86_64.zip) |
| macOS Apple Silicon | [t2w-macos-aarch64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-aarch64.tar.gz) |
| macOS Intel | [t2w-macos-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-x86_64.tar.gz) |
| Linux x64 | [t2w-linux-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-linux-x86_64.tar.gz) |

Each release also includes `.sha256` checksum files.

Windows:

```powershell
Expand-Archive .\t2w-windows-x86_64.zip -DestinationPath .
cd .\t2w-windows-x86_64
.\t2w.exe --provider mock --no-open "build a clean operations dashboard as HTML"
.\t2w-studio.exe --port 3000
```

macOS / Linux:

```bash
tar -xzf t2w-linux-x86_64.tar.gz
cd t2w-linux-x86_64
chmod +x t2w t2w-studio
./t2w --provider mock --no-open "build a clean operations dashboard as HTML"
./t2w-studio --port 3000
```

Use the matching archive name on macOS, for example `t2w-macos-aarch64.tar.gz` or `t2w-macos-x86_64.tar.gz`.

Then open [http://127.0.0.1:3000/](http://127.0.0.1:3000/).

## Quick Start From Source

Build the workspace:

```powershell
cargo build
```

Run the CLI without installing:

```powershell
cargo run -p agent-cli --bin t2w -- --provider mock --no-open "build a clean operations dashboard as HTML"
```

Install the binaries locally:

```powershell
cargo install --path crates/agent-cli --bin t2w --bin t2w-studio --force
```

Run Studio from source:

```powershell
cargo run -p agent-cli --bin t2w-studio -- --port 3000
```

## Anthropic Provider

For real generation, set an Anthropic key and model:

```powershell
$env:T2W_ANTHROPIC_API_KEY = "your-key"
$env:T2W_ANTHROPIC_MODEL = "claude-sonnet-4-5"
```

Mock mode does not require credentials.

## CLI Reference

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

Pipe data through stdin:

```powershell
Get-Content .\access.log | t2w "build an incident dashboard from these logs"
```

Default snapshot output is written under `.t2w/artifacts/` unless `--no-snapshot` is used.

## Studio Notes

- `GET /` serves the Studio shell.
- `GET /studio.css` and `GET /studio.js` serve embedded same-origin assets.
- API routes live under `/sessions`, `/runs`, `/templates`, and `/skills`.
- v1 allows one active run at a time; a second concurrent run returns `409 active_run_exists`.
- Studio sessions and runs are in-memory process state. Refreshing the page reloads from the current process; restarting the server clears them.

## Skills

v1 supports local skill discovery and prompt injection.

- Default search roots:
  - `./skills`
  - `./.t2w/skills`
- Extra roots can be added with `--skills-dir`.
- Explicit activation uses `--skill <name>`.
- Automatic activation order:
  - explicit name
  - `trigger` match
  - `description` token overlap

Example skill: [`examples/skills/log-dashboard/SKILL.md`](examples/skills/log-dashboard/SKILL.md)

## Releases

Cross-platform packages are built by [`.github/workflows/release.yml`](.github/workflows/release.yml).

Create a tagged release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The workflow builds and uploads:

- `t2w-windows-x86_64.zip`
- `t2w-macos-aarch64.tar.gz`
- `t2w-macos-x86_64.tar.gz`
- `t2w-linux-x86_64.tar.gz`
- matching `.sha256` files

## Roadmap

### v0.1.x - Portable Release Line

- Keep the CLI and Studio v1 contracts stable.
- Publish portable Windows, macOS, and Linux archives.
- Improve README screenshots, examples, and release instructions.

### v0.2 - Reinforcement Formula

The next product focus is the "reinforcement formula": a repeatable generation recipe that makes artifact quality less dependent on a single raw prompt.

Planned pieces:

- prompt formula: instruction, role, data contract, output contract, and visual constraints
- data formula: normalize stdin into typed context blocks before provider calls
- theme formula: map domain intent to stable layout, typography, density, and color decisions
- run formula: score generated HTML, detect missing requirements, and repair weak outputs
- formula presets for logs, tables, dashboards, reports, and inspectors

### v0.3 - Studio Persistence And Providers

- Persist Studio sessions and run history across restarts.
- Add more providers beyond `mock` and `anthropic`.
- Add provider-level model presets and safer credential handling.

### v0.4 - Skills And TUI

- Expand `SKILL.md` workflows beyond local discovery.
- Add installable skill packs and sandbox boundaries.
- Build the real TUI layer next to the browser Studio shell.

### v1.0 - Stable Local Artifact Workbench

- Stable CLI, Studio API, artifact shell, and release packages.
- Signed or native installers after the portable package lane proves itself.
- Strong docs, examples, screenshots, and upgrade notes.

## Development

Quality gates:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

Optional targeted checks:

```powershell
cargo test -p agent-cli --test live_preview
cargo test -p agent-cli --test studio_server
```

Or via `xtask`:

```powershell
cargo run -p xtask -- fmt
cargo run -p xtask -- clippy
cargo run -p xtask -- test
cargo run -p xtask -- smoke
```

## License

See [LICENSE](LICENSE).
