[English](README.md) | [中文](README_zh.md)

<p align="center">
  <img src="docs/assets/t2w-logo.png" alt="t2w logo" width="160">
</p>

<h1 align="center">t2w</h1>

<p align="center">
  Turn terminal input into local, inspectable HTML artifacts.
</p>

<p align="center">
  <a href="#getting-started">Getting Started</a>
  ·
  <a href="#studio">Studio</a>
  ·
  <a href="#download">Download</a>
  ·
  <a href="#faq">FAQ</a>
</p>

`t2w` is a Rust CLI and local browser Studio for turning prompts, logs, tables, notes, and stdin payloads into self-contained HTML pages. It streams output into a live preview, wraps finished HTML in an artifact shell, and keeps the default path local and token-free.

## Preview

| Artifact Shell | Studio |
| --- | --- |
| ![Artifact Shell](docs/assets/t2w-artifact-shell.png) | ![Studio](docs/assets/t2w-studio-shell.png) |

## Highlights

- **Local by default**: the `mock` provider is the default, so smoke tests and Studio checks need no key and spend no tokens.
- **Live HTML preview**: generated chunks stream into the browser while a run is still active.
- **Studio workspace**: manage sessions, templates, skills, run state, cancellation, source view, and downloads.
- **Persistent runs**: Studio stores sessions and completed run history under `.t2w/studio-state.json`.
- **Reinforcement Formula**: Prompt, Data, Theme, and Run stages are rendered locally in Studio and artifact shells.
- **Skill-aware prompting**: select local `SKILL.md` files explicitly or discover them from configured roots.
- **Portable releases**: release archives contain both binaries, README files, license, and bundled assets.

## Getting Started

Run the CLI from source:

```powershell
cargo run -p agent-cli --bin t2w -- --provider mock --no-open "build a clean operations dashboard as HTML"
```

Start Studio:

```powershell
cargo run -p agent-cli --bin t2w-studio -- --port 3000
```

Open [http://127.0.0.1:3000/](http://127.0.0.1:3000/).

Install both binaries locally:

```powershell
cargo install --path crates/agent-cli --bin t2w --bin t2w-studio --force
```

## CLI

Create an artifact from a plain instruction:

```powershell
t2w --provider mock --no-open "build a compact release checklist as HTML"
```

Pipe data into the instruction:

```powershell
Get-Content .\access.log | t2w "build an incident dashboard from these logs"
```

Use a local skill:

```powershell
t2w --skill log-dashboard "build a dashboard from these logs"
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

Snapshots are written to `.t2w/artifacts/` unless `--no-snapshot` is used.

## Studio

Studio is served locally by `t2w-studio`; it does not require a frontend build or hosted service.

```powershell
t2w-studio --port 3000
```

Options:

```text
t2w-studio
  --host <host>
  --port <port>
  --skills-dir <path>
  --config <path>
  --artifacts-dir <path>
  --sessions-file <path>
```

Default state and output paths:

| Path | Purpose |
| --- | --- |
| `.t2w/studio-state.json` | Studio sessions and run history. |
| `.t2w/artifacts/` | CLI and Studio snapshots. |

If the Studio state file is corrupt, t2w preserves the bad file as `.corrupt` and starts with an empty state.

## Providers And Cost

`mock` is the default provider. It is deterministic, local, and does not call a model.

Anthropic generation is opt-in. Set the enable flag and key, then choose the Anthropic provider:

```powershell
$env:T2W_ENABLE_ANTHROPIC = "1"
$env:T2W_ANTHROPIC_API_KEY = "your-key"
$env:T2W_ANTHROPIC_MODEL = "claude-sonnet-4-5"
t2w --provider anthropic "build a clean operations dashboard as HTML"
```

Equivalent config file:

```toml
anthropic_enabled = true
anthropic_api_key = "your-key"
anthropic_model = "claude-sonnet-4-5"
mock_chunk_delay_ms = 0
```

Environment variables:

| Name | Purpose |
| --- | --- |
| `T2W_ARTIFACTS_DIR` | Override snapshot output directory. |
| `T2W_ENABLE_ANTHROPIC` | Enable Anthropic when set to `1`, `true`, `yes`, or `on`. |
| `T2W_ANTHROPIC_ENABLED` | Alternate Anthropic enable flag. |
| `T2W_ANTHROPIC_API_KEY` | Anthropic API key. |
| `T2W_ANTHROPIC_MODEL` | Anthropic model name. |
| `T2W_MOCK_CHUNK_DELAY_MS` | Delay mock chunks for streaming tests. |

Only provider calls spend tokens. Mock runs, Studio rendering, previews, snapshots, and Reinforcement Formula display are local.

## Skills

t2w discovers local `SKILL.md` files from:

- `./skills`
- `./.t2w/skills`
- extra directories passed with `--skills-dir`

Example skill: [`examples/skills/log-dashboard/SKILL.md`](examples/skills/log-dashboard/SKILL.md)

## Supported Systems

The release workflow builds portable archives for:

| System | Architecture | Package |
| --- | --- | --- |
| Windows | x86_64 | `.zip` |
| macOS | Apple Silicon / aarch64 | `.tar.gz` |
| macOS | Intel / x86_64 | `.tar.gz` |
| Linux | x86_64 | `.tar.gz` |

t2w does not ship native `.msi`, `.pkg`, `.deb`, or AppImage installers yet. Current releases are portable archives.

## Download

Download the latest portable archive from [GitHub Releases](https://github.com/capwitf/t2w/releases/latest).

| Platform | Package |
| --- | --- |
| Windows x64 | [t2w-windows-x86_64.zip](https://github.com/capwitf/t2w/releases/latest/download/t2w-windows-x86_64.zip) |
| macOS Apple Silicon | [t2w-macos-aarch64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-aarch64.tar.gz) |
| macOS Intel | [t2w-macos-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-x86_64.tar.gz) |
| Linux x64 | [t2w-linux-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-linux-x86_64.tar.gz) |

Each release includes matching `.sha256` checksum files.

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

## FAQ

### Does t2w spend tokens by default?

No. The default `mock` provider is local and deterministic.

### When is an API key required?

Only when you explicitly enable Anthropic and select the `anthropic` provider.

### Is the Reinforcement Formula sent to the model?

No. It is rendered locally in Studio and artifact shells unless a future change explicitly wires it into provider prompting.

### What is `t2w-studio` in smoke tests?

`t2w-studio` is the local browser Studio binary. Smoke tests start it to verify the app shell, API, streaming preview, and artifact routes without requiring a model key.

### Where are generated files stored?

Snapshots go to `.t2w/artifacts/` by default. Studio state goes to `.t2w/studio-state.json`.

## Roadmap

- Stabilize the CLI, Studio API, artifact shell, and release archive contract.
- Expand Reinforcement Formula presets for logs, tables, dashboards, reports, and inspectors.
- Add more providers and model presets while keeping the local-first default.
- Add installable skill packs and stronger sandbox boundaries.
- Build a real TUI surface next to the browser Studio.

## Contact

Email: [capwitf@outlook.com](mailto:capwitf@outlook.com)

## License

MIT. See [LICENSE](LICENSE).

<p align="center">
  <img src="docs/assets/capwitf.png" alt="Capwitf signature" width="240">
</p>
