[English](README.md) | [中文](README_zh.md)

# t2w

把终端输入变成可预览、可检查、可保存的本地 HTML artifact。

`t2w` 是一个 Rust workspace，提供两个入口：一次性 CLI 和本地浏览器 Studio。它会把生成中的 HTML 流式送进实时预览，把最终结果包进 artifact shell，并可保存为自包含的本地 HTML 文件。

## 预览

| Artifact Shell | Studio |
| --- | --- |
| ![Artifact Shell](docs/assets/t2w-artifact-shell.png) | ![Studio](docs/assets/t2w-studio-shell.png) |

## 特性

- 本地优先的 CLI：把 `stdin + instruction` 转成 HTML。
- provider 流式输出时实时预览。
- Studio 支持 session、template、skill、run 状态、取消、预览、源码查看和下载。
- session 和 run 历史默认持久化到 `.t2w/studio-state.json`。
- 强化公式视图：展示 Prompt、Data、Theme、Run 四个阶段。这个视图在本地渲染，本身不增加 token 消耗。
- 默认使用确定性的 `mock` provider，不需要 API key，不消耗 token。
- Anthropic provider 只有显式开启后才可用。
- 从 `./skills`、`./.t2w/skills` 或 `--skills-dir` 发现本地 `SKILL.md` 并注入 prompt。
- Studio 资源嵌入 Rust server；不需要 Node、前端构建、CDN、远程字体或托管服务。

## 快速开始

从源码运行 CLI：

```powershell
cargo run -p agent-cli --bin t2w -- --provider mock --no-open "build a clean operations dashboard as HTML"
```

从源码启动 Studio：

```powershell
cargo run -p agent-cli --bin t2w-studio -- --port 3000
```

然后打开 [http://127.0.0.1:3000/](http://127.0.0.1:3000/)。

从源码安装两个二进制：

```powershell
cargo install --path crates/agent-cli --bin t2w --bin t2w-studio --force
```

## CLI

把文件传给 prompt：

```powershell
Get-Content .\access.log | t2w "build an incident dashboard from these logs"
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

默认 snapshot 输出到 `.t2w/artifacts/`。可用 `T2W_ARTIFACTS_DIR` 或 `--artifacts-dir` 修改输出目录。

## Studio

Studio 是由 `t2w-studio` 启动的本地同源 Web 应用。

```powershell
t2w-studio --port 3000
```

常用参数：

```text
t2w-studio
  --host <host>
  --port <port>
  --skills-dir <path>
  --config <path>
  --artifacts-dir <path>
  --sessions-file <path>
```

Studio 默认把 session 和已完成 run 历史保存到 `.t2w/studio-state.json`。如果状态文件损坏，Studio 会把原文件保留为 `.corrupt` 文件，并用空状态继续启动。

## Provider 和费用

默认 provider 是 `mock`。它是本地、确定性的，不会调用模型。

Anthropic 生成是显式开启的。你必须开启开关，并选择 Anthropic provider：

```powershell
$env:T2W_ENABLE_ANTHROPIC = "1"
$env:T2W_ANTHROPIC_API_KEY = "your-key"
$env:T2W_ANTHROPIC_MODEL = "claude-sonnet-4-5"
t2w --provider anthropic "build a clean operations dashboard as HTML"
```

等价配置项：

```toml
anthropic_enabled = true
anthropic_api_key = "your-key"
anthropic_model = "claude-sonnet-4-5"
mock_chunk_delay_ms = 0
```

只有 provider 调用会消耗 token。本地 Studio 渲染、mock run、预览、snapshot、强化公式展示都不消耗 token。

## Skills

Skill 是本地 `SKILL.md` 文件。t2w 会从这些位置发现：

- `./skills`
- `./.t2w/skills`
- 通过 `--skills-dir` 传入的额外目录

显式启用：

```powershell
t2w --skill log-dashboard "build a dashboard from these logs"
```

示例：[`examples/skills/log-dashboard/SKILL.md`](examples/skills/log-dashboard/SKILL.md)

## 支持系统

当前 release workflow 会为这些系统构建便携包：

| 系统 | 架构 | 包格式 |
| --- | --- | --- |
| Windows | x86_64 | `.zip` |
| macOS | Apple Silicon / aarch64 | `.tar.gz` |
| macOS | Intel / x86_64 | `.tar.gz` |
| Linux | x86_64 | `.tar.gz` |

项目目前还没有 `.msi`、`.pkg`、`.deb` 或 AppImage 这类原生安装器。当前发布形态是便携压缩包，里面包含 `t2w`、`t2w-studio`、README、LICENSE 和内置资源。

## 下载

GitHub Releases 提供便携包。

| 平台 | 包 |
| --- | --- |
| Windows x64 | [t2w-windows-x86_64.zip](https://github.com/capwitf/t2w/releases/latest/download/t2w-windows-x86_64.zip) |
| macOS Apple Silicon | [t2w-macos-aarch64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-aarch64.tar.gz) |
| macOS Intel | [t2w-macos-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-x86_64.tar.gz) |
| Linux x64 | [t2w-linux-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-linux-x86_64.tar.gz) |

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

每个 release 都包含对应的 `.sha256` 校验文件。

## 开发

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

发行包由 [`.github/workflows/release.yml`](.github/workflows/release.yml) 构建：

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 路线图

- 稳定 CLI、Studio API、artifact shell 和 release archive 合约。
- 继续把强化公式扩展成 template-specific 的生成配方。
- 增加更多 provider 和模型预设，但保持本地优先的默认行为。
- 扩展可安装 skill packs 和 sandbox 边界。
- 在浏览器 Studio 之外补齐真正的 TUI。

## License

MIT. 见 [LICENSE](LICENSE)。
