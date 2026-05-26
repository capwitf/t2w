[English](README.md) | [中文](README_zh.md)

# t2w

Turn terminal input into a local, inspectable HTML artifact.

`t2w` is a Rust workspace with two entry points: a one-shot CLI and a local browser Studio. It streams generated HTML into a live preview, wraps the result in an artifact shell, and can save the final page as a self-contained local file.

## Preview

| Artifact Shell | Studio |
| --- | --- |
| ![Artifact Shell](docs/assets/t2w-artifact-shell.png) | ![Studio](docs/assets/t2w-studio-shell.png) |

## Features

- Local-first CLI for turning `stdin + instruction` into HTML.
- Live preview while the provider streams output.
- Studio UI for sessions, templates, skills, run status, cancellation, preview, source view, and download.
- Local session and run persistence in `.t2w/studio-state.json`.
- Reinforcement Formula view for Prompt, Data, Theme, and Run stages. This is rendered locally and does not add tokens by itself.
- Deterministic `mock` provider by default, with no API key and no token spend.
- Anthropic provider is available only when explicitly enabled with an environment variable or config.
- Local `SKILL.md` discovery and prompt injection from `./skills`, `./.t2w/skills`, or `--skills-dir`.
- Embedded Studio assets. No Node, frontend build step, CDN, remote fonts, or hosted service is required.

## Quick Start

Run the CLI from source:

```powershell
cargo run -p agent-cli --bin t2w -- --provider mock --no-open "build a clean operations dashboard as HTML"
```

Start Studio from source:

```powershell
cargo run -p agent-cli --bin t2w-studio -- --port 3000
```

Then open [http://127.0.0.1:3000/](http://127.0.0.1:3000/).

Install both binaries from source:

```powershell
cargo install --path crates/agent-cli --bin t2w --bin t2w-studio --force
```

## CLI

Pipe data into a prompt:

```powershell
Get-Content .\access.log | t2w "build an incident dashboard from these logs"
```

Common options:

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

Snapshots are written to `.t2w/artifacts/` unless `--no-snapshot` is used. Set `T2W_ARTIFACTS_DIR` or pass `--artifacts-dir` to change the output directory.

## Studio

Studio runs as a local same-origin app served by `t2w-studio`.

```powershell
t2w-studio --port 3000
```

Useful options:

```text
t2w-studio
  --host <host>
  --port <port>
  --skills-dir <path>
  --config <path>
  --artifacts-dir <path>
  --sessions-file <path>
```

By default, Studio stores sessions and completed run history in `.t2w/studio-state.json`. If that state file is corrupt, Studio preserves it as a `.corrupt` file and starts with an empty state.

## Providers And Cost

The default provider is `mock`. It is local, deterministic, and does not call a model.

Anthropic generation is opt-in. You must both enable it and select the Anthropic provider:

```powershell
$env:T2W_ENABLE_ANTHROPIC = "1"
$env:T2W_ANTHROPIC_API_KEY = "your-key"
$env:T2W_ANTHROPIC_MODEL = "claude-sonnet-4-5"
t2w --provider anthropic "build a clean operations dashboard as HTML"
```

Equivalent config keys:

```toml
anthropic_enabled = true
anthropic_api_key = "your-key"
anthropic_model = "claude-sonnet-4-5"
mock_chunk_delay_ms = 0
```

Token use comes only from provider calls. Local Studio rendering, mock runs, previews, snapshots, and the Reinforcement Formula display do not spend tokens.

## Skills

Skills are local `SKILL.md` files. t2w discovers them from:

- `./skills`
- `./.t2w/skills`
- extra paths passed with `--skills-dir`

Explicit activation:

```powershell
t2w --skill log-dashboard "build a dashboard from these logs"
```

Example: [`examples/skills/log-dashboard/SKILL.md`](examples/skills/log-dashboard/SKILL.md)

## Supported Systems

The release workflow currently builds portable archives for these systems:

| System | Architecture | Package |
| --- | --- | --- |
| Windows | x86_64 | `.zip` |
| macOS | Apple Silicon / aarch64 | `.tar.gz` |
| macOS | Intel / x86_64 | `.tar.gz` |
| Linux | x86_64 | `.tar.gz` |

This repository does not ship native installers such as `.msi`, `.pkg`, `.deb`, or AppImage yet. The current release format is a portable archive containing `t2w`, `t2w-studio`, README files, license, and bundled assets.

## Download

Portable archives are published from GitHub Releases.

| Platform | Package |
| --- | --- |
| Windows x64 | [t2w-windows-x86_64.zip](https://github.com/capwitf/t2w/releases/latest/download/t2w-windows-x86_64.zip) |
| macOS Apple Silicon | [t2w-macos-aarch64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-aarch64.tar.gz) |
| macOS Intel | [t2w-macos-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-x86_64.tar.gz) |
| Linux x64 | [t2w-linux-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-linux-x86_64.tar.gz) |

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

Each release includes matching `.sha256` checksum files.

## Development

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

Release packages are built by [`.github/workflows/release.yml`](.github/workflows/release.yml):

```bash
git tag v0.1.0
git push origin v0.1.0
```

## Roadmap

- Stabilize the CLI, Studio API, artifact shell, and release archive contract.
- Keep expanding the Reinforcement Formula into template-specific generation recipes.
- Add more providers and model presets without changing the local-first default.
- Expand installable skill packs and sandbox boundaries.
- Build the real TUI surface next to the browser Studio.

## License

MIT. See [LICENSE](LICENSE).
