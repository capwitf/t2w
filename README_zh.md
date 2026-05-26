[English](README.md) | [中文](README_zh.md)

<p align="center">
  <img src="docs/assets/t2w-logo.png" alt="t2w logo" width="160">
</p>

<h1 align="center">t2w</h1>

<p align="center">
  把终端输入转换成可本地查看、可检查的 HTML artifact。
</p>

<p align="center">
  <a href="#快速开始">快速开始</a>
  ·
  <a href="#studio">Studio</a>
  ·
  <a href="#下载">下载</a>
  ·
  <a href="#faq">FAQ</a>
</p>

`t2w` 是一个 Rust CLI 和本地浏览器 Studio。它可以把 prompt、日志、表格、笔记和 stdin 数据转换成自包含 HTML 页面。生成过程会流式进入实时预览，完成后会包进 artifact shell。默认路径是本地的，不需要 key，也不消耗 token。

## 预览

| Artifact Shell | Studio |
| --- | --- |
| ![Artifact Shell](docs/assets/t2w-artifact-shell.png) | ![Studio](docs/assets/t2w-studio-shell.png) |

## 功能亮点

- **默认本地运行**：默认 provider 是 `mock`，smoke test 和 Studio 检查不需要 key，也不消耗 token。
- **实时 HTML 预览**：生成中的 chunk 会流式进入浏览器页面。
- **Studio 工作区**：管理 session、template、skill、运行状态、取消、源码视图和下载。
- **运行记录持久化**：Studio 会把 session 和已完成 run 记录保存到 `.t2w/studio-state.json`。
- **Reinforcement Formula**：Prompt、Data、Theme、Run 阶段在 Studio 和 artifact shell 中本地渲染。
- **Skill 感知 prompt**：可以显式选择本地 `SKILL.md`，也可以从配置目录自动发现。
- **便携发布包**：release archive 内含两个二进制文件、README、license 和 bundled assets。

## 快速开始

从源码运行 CLI：

```powershell
cargo run -p agent-cli --bin t2w -- --provider mock --no-open "build a clean operations dashboard as HTML"
```

启动 Studio：

```powershell
cargo run -p agent-cli --bin t2w-studio -- --port 3000
```

打开 [http://127.0.0.1:3000/](http://127.0.0.1:3000/)。

把两个二进制安装到本机：

```powershell
cargo install --path crates/agent-cli --bin t2w --bin t2w-studio --force
```

## CLI

用普通指令生成 artifact：

```powershell
t2w --provider mock --no-open "build a compact release checklist as HTML"
```

把数据通过管道传入：

```powershell
Get-Content .\access.log | t2w "build an incident dashboard from these logs"
```

使用本地 skill：

```powershell
t2w --skill log-dashboard "build a dashboard from these logs"
```

常用参数：

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

未使用 `--no-snapshot` 时，snapshot 默认写入 `.t2w/artifacts/`。

## Studio

Studio 由 `t2w-studio` 在本地提供服务；不需要前端构建，也不依赖托管服务。

```powershell
t2w-studio --port 3000
```

参数：

```text
t2w-studio
  --host <host>
  --port <port>
  --skills-dir <path>
  --config <path>
  --artifacts-dir <path>
  --sessions-file <path>
```

默认状态和输出路径：

| 路径 | 用途 |
| --- | --- |
| `.t2w/studio-state.json` | Studio session 和 run 历史。 |
| `.t2w/artifacts/` | CLI 和 Studio snapshot。 |

如果 Studio 状态文件损坏，t2w 会把坏文件保留为 `.corrupt`，然后用空状态启动。

## Provider 和成本

`mock` 是默认 provider。它是确定性的、本地的，不会调用模型。

Anthropic 生成是可选能力。需要先打开启用开关并设置 key，然后显式选择 Anthropic provider：

```powershell
$env:T2W_ENABLE_ANTHROPIC = "1"
$env:T2W_ANTHROPIC_API_KEY = "your-key"
$env:T2W_ANTHROPIC_MODEL = "claude-sonnet-4-5"
t2w --provider anthropic "build a clean operations dashboard as HTML"
```

等价配置文件：

```toml
anthropic_enabled = true
anthropic_api_key = "your-key"
anthropic_model = "claude-sonnet-4-5"
mock_chunk_delay_ms = 0
```

环境变量：

| 名称 | 用途 |
| --- | --- |
| `T2W_ARTIFACTS_DIR` | 覆盖 snapshot 输出目录。 |
| `T2W_ENABLE_ANTHROPIC` | 设置为 `1`、`true`、`yes` 或 `on` 时启用 Anthropic。 |
| `T2W_ANTHROPIC_ENABLED` | 备用 Anthropic 启用开关。 |
| `T2W_ANTHROPIC_API_KEY` | Anthropic API key。 |
| `T2W_ANTHROPIC_MODEL` | Anthropic model 名称。 |
| `T2W_MOCK_CHUNK_DELAY_MS` | 为流式测试延迟 mock chunk。 |

只有 provider 调用会消耗 token。Mock 运行、Studio 渲染、预览、snapshot、Reinforcement Formula 展示都在本地完成。

## Skills

t2w 会从这些位置发现本地 `SKILL.md`：

- `./skills`
- `./.t2w/skills`
- 通过 `--skills-dir` 传入的额外目录

示例 skill：[`examples/skills/log-dashboard/SKILL.md`](examples/skills/log-dashboard/SKILL.md)

## 支持系统

release workflow 会构建这些便携 archive：

| 系统 | 架构 | 包格式 |
| --- | --- | --- |
| Windows | x86_64 | `.zip` |
| macOS | Apple Silicon / aarch64 | `.tar.gz` |
| macOS | Intel / x86_64 | `.tar.gz` |
| Linux | x86_64 | `.tar.gz` |

t2w 暂时不提供原生 `.msi`、`.pkg`、`.deb` 或 AppImage 安装器。当前 release 是便携 archive。

## 下载

从 [GitHub Releases](https://github.com/capwitf/t2w/releases/latest) 下载最新便携包。

| 平台 | 包 |
| --- | --- |
| Windows x64 | [t2w-windows-x86_64.zip](https://github.com/capwitf/t2w/releases/latest/download/t2w-windows-x86_64.zip) |
| macOS Apple Silicon | [t2w-macos-aarch64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-aarch64.tar.gz) |
| macOS Intel | [t2w-macos-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-x86_64.tar.gz) |
| Linux x64 | [t2w-linux-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-linux-x86_64.tar.gz) |

每个 release 都包含对应的 `.sha256` 校验文件。

Windows：

```powershell
Expand-Archive .\t2w-windows-x86_64.zip -DestinationPath .
cd .\t2w-windows-x86_64
.\t2w.exe --provider mock --no-open "build a clean operations dashboard as HTML"
.\t2w-studio.exe --port 3000
```

macOS / Linux：

```bash
tar -xzf t2w-linux-x86_64.tar.gz
cd t2w-linux-x86_64
chmod +x t2w t2w-studio
./t2w --provider mock --no-open "build a clean operations dashboard as HTML"
./t2w-studio --port 3000
```

## 开发

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

release 包由 [`.github/workflows/release.yml`](.github/workflows/release.yml) 构建：

```bash
git tag v0.1.0
git push origin v0.1.0
```

## FAQ

### t2w 默认会消耗 token 吗？

不会。默认 `mock` provider 是本地且确定性的。

### 什么时候需要 API key？

只有在你显式启用 Anthropic 并选择 `anthropic` provider 时才需要。

### Reinforcement Formula 会发给模型吗？

不会。它目前只在 Studio 和 artifact shell 中本地渲染，除非未来明确把它接入 provider prompt。

### smoke test 里的 `t2w-studio` 是什么？

`t2w-studio` 是本地浏览器 Studio 二进制。smoke test 会启动它，用来验证 app shell、API、流式预览和 artifact 路由，不需要模型 key。

### 生成文件放在哪里？

snapshot 默认进入 `.t2w/artifacts/`。Studio 状态默认进入 `.t2w/studio-state.json`。

## Roadmap

- 稳定 CLI、Studio API、artifact shell 和 release archive contract。
- 扩展面向日志、表格、dashboard、report 和 inspector 的 Reinforcement Formula preset。
- 增加更多 provider 和 model preset，同时保持 local-first 默认路径。
- 增加可安装 skill pack，并强化 sandbox 边界。
- 在浏览器 Studio 之外补上真正的 TUI 界面。

## 联系

Email: [capwitf@outlook.com](mailto:capwitf@outlook.com)

## License

MIT. See [LICENSE](LICENSE).

<p align="center">
  <img src="docs/assets/capwitf.png" alt="Capwitf signature" width="240">
</p>
