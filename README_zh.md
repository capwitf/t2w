[English](README.md) | [中文](README_zh.md)

# t2w

`t2w` 是一个 Rust 优先的本地 artifact 引擎：把终端输入和指令变成可实时预览、可保存、可下载的 HTML。它同时提供一次性 CLI 流程和本地 Studio 控制台。

## 预览

![t2w artifact shell](docs/assets/t2w-artifact-shell.png)

默认 artifact shell 会把生成出来的 HTML 包进一个本地工作台：Prompt、Data、Theme、Run、状态、全屏预览和 HTML 导出都在同一个页面里。

![t2w Studio shell](docs/assets/t2w-studio-shell.png)

Studio 由 `t2w-studio` 在本地启动，同源服务前端资源；不需要 Node 构建，不依赖 CDN、外部字体或远程托管服务。

## 亮点

- 把 `stdin + instruction` 流式生成到浏览器实时预览。
- 结束后保存自包含 `.html` artifact。
- 本地 Studio 支持 session、template、skill、运行状态、取消、预览和下载。
- `mock` provider 适合稳定冒烟测试，`anthropic` provider 用于真实生成。
- 自动发现本地 `SKILL.md`，并把相关技能说明注入 prompt。
- Studio 前端资源已经嵌进 Rust server，所以发行包只需要两个二进制。

## 安装

v1.x 先采用轻量便携包：下载、解压、运行。`.msi`、`.pkg`、`.deb`、AppImage 这类原生安装器会等 CLI 和 Studio 合约更稳定后再做。

| 平台 | 包 |
| --- | --- |
| Windows x64 | [t2w-windows-x86_64.zip](https://github.com/capwitf/t2w/releases/latest/download/t2w-windows-x86_64.zip) |
| macOS Apple Silicon | [t2w-macos-aarch64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-aarch64.tar.gz) |
| macOS Intel | [t2w-macos-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-macos-x86_64.tar.gz) |
| Linux x64 | [t2w-linux-x86_64.tar.gz](https://github.com/capwitf/t2w/releases/latest/download/t2w-linux-x86_64.tar.gz) |

每个 release 都会同时提供 `.sha256` 校验文件。

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

macOS 按机器选择对应包名，例如 `t2w-macos-aarch64.tar.gz` 或 `t2w-macos-x86_64.tar.gz`。

然后打开 [http://127.0.0.1:3000/](http://127.0.0.1:3000/)。

## 从源码快速开始

构建工作区：

```powershell
cargo build
```

不安装，直接跑 CLI：

```powershell
cargo run -p agent-cli --bin t2w -- --provider mock --no-open "build a clean operations dashboard as HTML"
```

安装本地二进制：

```powershell
cargo install --path crates/agent-cli --bin t2w --bin t2w-studio --force
```

从源码启动 Studio：

```powershell
cargo run -p agent-cli --bin t2w-studio -- --port 3000
```

## Anthropic Provider

真实生成需要配置 Anthropic key 和模型：

```powershell
$env:T2W_ANTHROPIC_API_KEY = "your-key"
$env:T2W_ANTHROPIC_MODEL = "claude-sonnet-4-5"
```

只做本地验证时用 `mock` 即可，不需要密钥。

## CLI 参数

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

通过 stdin 传数据：

```powershell
Get-Content .\access.log | t2w "build an incident dashboard from these logs"
```

默认 snapshot 会写入 `.t2w/artifacts/`，除非使用 `--no-snapshot`。

## Studio 说明

- `GET /` 提供 Studio 页面。
- `GET /studio.css` 和 `GET /studio.js` 提供嵌入式同源资源。
- API 路由位于 `/sessions`、`/runs`、`/templates`、`/skills`。
- v1 同时只允许一个 active run；第二个并发 run 会返回 `409 active_run_exists`。
- Studio 的 session 和 run 目前是进程内存状态。刷新页面会从当前进程恢复；重启服务后会清空。

## Skills

v1 支持本地 skill 发现和 prompt 注入。

- 默认搜索目录：
  - `./skills`
  - `./.t2w/skills`
- 可通过 `--skills-dir` 增加目录。
- 显式启用方式：`--skill <name>`。
- 自动匹配顺序：
  - 显式名称
  - `trigger` 匹配
  - `description` 词项重叠

示例 skill: [`examples/skills/log-dashboard/SKILL.md`](examples/skills/log-dashboard/SKILL.md)

## Release

跨平台便携包由 [`.github/workflows/release.yml`](.github/workflows/release.yml) 自动构建。

创建 tagged release：

```bash
git tag v0.1.0
git push origin v0.1.0
```

workflow 会构建并上传：

- `t2w-windows-x86_64.zip`
- `t2w-macos-aarch64.tar.gz`
- `t2w-macos-x86_64.tar.gz`
- `t2w-linux-x86_64.tar.gz`
- 对应的 `.sha256` 文件

## 版本规划

### v0.1.x - 便携发行线

- 稳住 CLI 和 Studio v1 合约。
- 发布 Windows、macOS、Linux 便携包。
- 完善 README 截图、示例和 release 指引。

### v0.2 - 强化公式

下一阶段主线是“强化公式”：把一次 raw prompt 变成可重复、可评分、可修复的生成配方，让 artifact 质量更稳定。

计划包含：

- prompt 公式：指令、角色、数据契约、输出契约、视觉约束
- data 公式：把 stdin 规范化为 typed context blocks 后再进 provider
- theme 公式：根据领域意图稳定映射布局、字体、密度和配色
- run 公式：评分生成 HTML，发现缺失要求，并自动修复弱输出
- 公式预设：logs、tables、dashboards、reports、inspectors

### v0.3 - Studio 持久化和 Provider

- Studio session 和 run history 跨重启持久化。
- 增加 `mock`、`anthropic` 之外的 provider。
- 增加 provider 模型预设和更安全的凭据处理。

### v0.4 - Skills 和 TUI

- 扩展 `SKILL.md` 工作流，不只停留在本地发现。
- 增加可安装 skill packs 和 sandbox 边界。
- 在浏览器 Studio 之外补齐真正的 TUI 层。

### v1.0 - 稳定本地 Artifact Workbench

- 稳定 CLI、Studio API、artifact shell 和 release 包。
- 便携包路线跑稳后，再做签名或原生安装器。
- 补齐文档、示例、截图和升级说明。

## 开发

质量门：

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

定向检查：

```powershell
cargo test -p agent-cli --test live_preview
cargo test -p agent-cli --test studio_server
```

也可以通过 `xtask`：

```powershell
cargo run -p xtask -- fmt
cargo run -p xtask -- clippy
cargo run -p xtask -- test
cargo run -p xtask -- smoke
```

## License

见 [LICENSE](LICENSE)。
