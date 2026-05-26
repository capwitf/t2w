use serde::Serialize;

use crate::studio::{FormulaStep, default_reinforcement_formula};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactShellContext {
    pub title: String,
    pub instruction: String,
    pub stdin: String,
    pub template_name: String,
    pub template_description: String,
    pub provider: String,
    pub model: String,
    pub formula: Vec<FormulaStep>,
}

impl ArtifactShellContext {
    pub fn new(
        title: impl Into<String>,
        instruction: impl Into<String>,
        stdin: impl Into<String>,
        template_name: impl Into<String>,
        template_description: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            instruction: instruction.into(),
            stdin: stdin.into(),
            template_name: template_name.into(),
            template_description: template_description.into(),
            provider: provider.into(),
            model: model.into(),
            formula: default_reinforcement_formula(),
        }
    }

    pub fn with_formula(mut self, formula: Vec<FormulaStep>) -> Self {
        self.formula = formula;
        self
    }
}

#[derive(Serialize)]
struct ArtifactShellMetadata<'a> {
    shell: &'static str,
    title: &'a str,
    instruction: &'a str,
    stdin: &'a str,
    template_name: &'a str,
    template_description: &'a str,
    provider: &'a str,
    model: &'a str,
    formula: &'a [FormulaStep],
}

pub fn artifact_title_from_instruction(instruction: &str) -> String {
    let title = instruction
        .split_whitespace()
        .take(10)
        .collect::<Vec<_>>()
        .join(" ");

    if title.is_empty() {
        "Untitled artifact".to_string()
    } else if title.len() > 72 {
        format!("{}...", title.chars().take(69).collect::<String>())
    } else {
        title
    }
}

pub fn render_artifact_shell(context: &ArtifactShellContext, artifact_html: &str) -> String {
    render_shell_document(context, &escape_html(artifact_html), "")
}

pub fn render_artifact_shell_stream_start(context: &ArtifactShellContext) -> String {
    render_shell_document(context, "", "")
}

pub fn render_artifact_shell_stream_chunk(chunk: &str) -> String {
    format!(
        r#"<script>window.__t2wAppendArtifactChunk && window.__t2wAppendArtifactChunk({});</script>"#,
        safe_script_json(&serde_json::to_string(chunk).unwrap_or_else(|_| "\"\"".to_string()))
    )
}

pub fn render_artifact_shell_stream_finish() -> String {
    r#"<script>window.__t2wMarkArtifactComplete && window.__t2wMarkArtifactComplete();</script>"#
        .to_string()
}

fn render_shell_document(
    context: &ArtifactShellContext,
    escaped_artifact_html: &str,
    extra_script: &str,
) -> String {
    let title = escape_html(&context.title);
    let instruction = render_pre_text(&context.instruction, "No instruction provided.");
    let stdin = render_pre_text(&context.stdin, "No stdin data provided.");
    let template_name = escape_html(&context.template_name);
    let template_description = escape_html(&context.template_description);
    let provider = escape_html(&context.provider);
    let model = escape_html(&context.model);
    let formula = render_formula(&context.formula);
    let metadata_json = shell_metadata_json(context);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} - t2w Artifact</title>
  <link rel="icon" href="data:,">
  <style>
    :root {{
      color-scheme: dark;
      --bg: #121314;
      --surface-lowest: #0d0e0f;
      --panel: #1f2021;
      --panel-high: #292a2b;
      --panel-higher: #343536;
      --line: #534434;
      --line-soft: #343536;
      --text: #e3e2e3;
      --muted: #d8c3ad;
      --quiet: #a08e7a;
      --accent: #f59e0b;
      --accent-strong: #ffc174;
      --accent-text: #271706;
      --preview: #e3e2e3;
      --preview-text: #303031;
      --error: #ffb4ab;
      --good: #f59e0b;
      --radius: 4px;
      --rail-width: 64px;
      --sidebar-width: 256px;
      --header-height: 56px;
    }}

    * {{ box-sizing: border-box; }}

    html,
    body {{
      margin: 0;
      width: 100%;
      min-width: 320px;
      min-height: 100%;
      background: var(--bg);
      color: var(--text);
      font-family: Aptos, Bahnschrift, "Segoe UI Variable", "Helvetica Neue", sans-serif;
    }}

    button {{
      border: 0;
      font: inherit;
      cursor: pointer;
    }}

    pre {{
      margin: 0;
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      font-family: "Cascadia Code", "JetBrains Mono", Consolas, monospace;
    }}

    .artifact-shell {{
      height: 100vh;
      display: grid;
      grid-template-columns: var(--rail-width) minmax(0, 1fr) var(--sidebar-width);
      grid-template-rows: var(--header-height) minmax(0, 1fr);
      overflow: hidden;
      animation: shell-in 420ms ease-out both;
    }}

    .rail {{
      grid-row: 1 / 3;
      border-right: 1px solid var(--line-soft);
      background: var(--surface-lowest);
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 8px;
      padding: 12px 0;
    }}

    .brand-dot {{
      width: 32px;
      height: 32px;
      display: grid;
      place-items: center;
      border-radius: var(--radius);
      background: var(--accent);
      color: var(--accent-text);
      font-weight: 800;
      letter-spacing: -0.04em;
      box-shadow: 0 0 18px rgba(245, 158, 11, 0.28);
    }}

    .rail-mark {{
      width: 100%;
      height: 48px;
      display: grid;
      place-items: center;
      border-radius: 0;
      color: var(--muted);
      border: 0;
      border-left: 3px solid transparent;
      font-size: 18px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }}

    .rail-mark.is-active {{
      color: var(--accent-strong);
      border-left-color: var(--accent);
      background: #121314;
    }}

    .topbar {{
      grid-column: 2 / 4;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding: 0 16px 0 20px;
      border-bottom: 1px solid var(--line-soft);
      background: var(--panel);
      backdrop-filter: blur(18px);
    }}

    .topbar-title {{
      min-width: 0;
      display: flex;
      align-items: center;
      gap: 16px;
    }}

    .product {{
      color: var(--accent-strong);
      font-weight: 800;
      letter-spacing: -0.03em;
    }}

    .divider {{
      width: 1px;
      height: 24px;
      background: var(--line);
    }}

    .artifact-name {{
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-weight: 650;
    }}

    .status-strip {{
      display: flex;
      align-items: center;
      gap: 12px;
      color: var(--muted);
      font-size: 12px;
    }}

    .topbar-actions {{
      display: flex;
      align-items: center;
      gap: 12px;
    }}

    .status-pill {{
      display: inline-flex;
      align-items: center;
      gap: 8px;
      min-height: 28px;
      padding: 0 4px 0 0;
      border: 0;
      border-radius: 0;
      background: transparent;
      color: var(--text);
      font-family: "Cascadia Code", "JetBrains Mono", Consolas, monospace;
      font-size: 12px;
    }}

    .status-pill::before {{
      content: "";
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--good);
      box-shadow: 0 0 10px rgba(245, 158, 11, 0.62);
    }}

    body[data-run-status="failed"] .status-pill::before {{
      background: var(--error);
      box-shadow: 0 0 10px rgba(255, 180, 171, 0.42);
    }}

    .canvas-pane {{
      grid-column: 2;
      grid-row: 2;
      min-width: 0;
      min-height: 0;
      display: flex;
      flex-direction: column;
      gap: 20px;
      padding: 24px;
      background: var(--surface-lowest);
    }}

    .canvas-header {{
      display: flex;
      align-items: end;
      justify-content: space-between;
      gap: 18px;
    }}

    .eyebrow {{
      margin: 0 0 4px;
      color: var(--muted);
      font-size: 11px;
      line-height: 1.2;
      letter-spacing: 0.11em;
      text-transform: uppercase;
    }}

    h1,
    h2,
    p {{
      margin: 0;
    }}

    h1 {{
      max-width: 840px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-size: clamp(22px, 3vw, 34px);
      line-height: 1.08;
      letter-spacing: -0.045em;
    }}

    .subtle {{
      margin-top: 8px;
      color: var(--muted);
      font-size: 16px;
    }}

    .canvas-tools {{
      display: flex;
      align-items: center;
      gap: 8px;
    }}

    .preview-frame-wrap {{
      position: relative;
      flex: 1;
      min-height: 0;
      border: 1px solid var(--line-soft);
      border-radius: 8px;
      overflow: hidden;
      background: var(--preview);
      animation: preview-in 500ms 80ms ease-out both;
    }}

    .preview-label {{
      position: absolute;
      top: 8px;
      right: 8px;
      z-index: 2;
      padding: 5px 9px;
      border: 1px solid rgba(56, 65, 78, 0.72);
      border-radius: var(--radius);
      background: rgba(18, 19, 20, 0.84);
      color: var(--muted);
      font-size: 11px;
      backdrop-filter: blur(10px);
    }}

    #artifact-frame {{
      width: 100%;
      height: 100%;
      border: 0;
      background: white;
      color-scheme: light;
    }}

    .console-pane {{
      grid-column: 3;
      grid-row: 2;
      min-width: 0;
      min-height: 0;
      display: flex;
      flex-direction: column;
      border-left: 1px solid var(--line-soft);
      background: var(--bg);
    }}

    .console-head {{
      padding: 16px 20px 14px;
      border-bottom: 1px solid var(--line-soft);
    }}

    .console-head h2 {{
      font-size: 20px;
      letter-spacing: -0.03em;
    }}

    .console-head .subtle {{
      margin-top: 2px;
      font-size: 12px;
    }}

    .tabs {{
      display: grid;
      grid-template-columns: repeat(5, 1fr);
      border-bottom: 1px solid var(--line-soft);
    }}

    .tab {{
      min-height: 80px;
      display: grid;
      place-items: center;
      gap: 4px;
      background: transparent;
      color: var(--muted);
      border-bottom: 2px solid transparent;
      font-family: "Cascadia Code", "JetBrains Mono", Consolas, monospace;
      font-size: 13px;
      transition: color 150ms ease, background 150ms ease, border-color 150ms ease;
    }}

    .tab span:first-child {{
      font-size: 15px;
    }}

    .tab:hover,
    .tab[aria-selected="true"] {{
      color: var(--text);
      background: rgba(255, 255, 255, 0.03);
    }}

    .tab[aria-selected="true"] {{
      border-bottom-color: var(--accent);
    }}

    .panel {{
      display: none;
      min-height: 0;
      overflow: auto;
      padding: 20px;
    }}

    .panel.is-active {{
      display: block;
    }}

    .readout {{
      margin-top: 10px;
      padding: 12px;
      border: 1px solid var(--line-soft);
      border-radius: var(--radius);
      background: var(--panel-high);
      color: var(--text);
      font-size: 12px;
      line-height: 1.55;
    }}

    .console-live-frame-wrap {{
      margin-top: 10px;
      height: min(48vh, 460px);
      min-height: 280px;
      border: 1px solid var(--line-soft);
      border-radius: var(--radius);
      overflow: hidden;
      background: var(--preview);
    }}

    #console-live-frame {{
      width: 100%;
      height: 100%;
      border: 0;
      background: white;
      color-scheme: light;
    }}

    .meta-list {{
      display: grid;
      gap: 10px;
      margin-top: 12px;
    }}

    .meta-row {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding-bottom: 10px;
      border-bottom: 1px solid var(--line-soft);
      color: var(--muted);
      font-size: 12px;
    }}

    .meta-row strong {{
      color: var(--text);
      font-weight: 650;
      text-align: right;
      overflow-wrap: anywhere;
    }}

    .formula-list {{
      display: grid;
      gap: 10px;
      margin-top: 12px;
    }}

    .formula-stage {{
      display: grid;
      gap: 8px;
      padding: 12px;
      border: 1px solid var(--line-soft);
      border-radius: var(--radius);
      background: var(--panel-high);
    }}

    .formula-stage-head {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
    }}

    .formula-stage-head span {{
      color: var(--accent-soft);
      font-size: 11px;
      font-family: var(--mono);
      text-transform: uppercase;
    }}

    .formula-stage-head strong {{
      color: var(--text);
      font-size: 13px;
      text-align: right;
    }}

    .formula-stage p {{
      margin: 0;
      color: var(--muted);
      font-size: 12px;
      line-height: 1.45;
    }}

    .formula-stage ul {{
      display: grid;
      gap: 6px;
      margin: 0;
      padding-left: 16px;
      color: var(--text);
      font-size: 12px;
      line-height: 1.45;
    }}

    .actions {{
      display: grid;
      gap: 10px;
      margin-top: 16px;
    }}

    .action-button {{
      min-height: 40px;
      border: 1px solid var(--line-soft);
      border-radius: var(--radius);
      background: transparent;
      color: var(--text);
      transition: transform 150ms ease, border-color 150ms ease, background 150ms ease;
    }}

    .action-button:hover {{
      transform: translateY(-1px);
      border-color: var(--accent);
      background: var(--panel-high);
    }}

    .action-button.primary {{
      background: var(--accent);
      border-color: var(--accent);
      color: var(--accent-text);
      font-weight: 700;
    }}

    @keyframes shell-in {{
      from {{ opacity: 0; transform: translateY(8px); }}
      to {{ opacity: 1; transform: translateY(0); }}
    }}

    @keyframes preview-in {{
      from {{ opacity: 0; transform: scale(0.985); }}
      to {{ opacity: 1; transform: scale(1); }}
    }}

    @media (max-width: 980px) {{
      .artifact-shell {{
        height: auto;
        min-height: 100vh;
        grid-template-columns: minmax(0, 1fr);
        grid-template-rows: auto auto auto;
        overflow: visible;
      }}

      .rail {{
        display: none;
      }}

      .topbar,
      .canvas-pane,
      .console-pane {{
        grid-column: 1;
        grid-row: auto;
      }}

      .topbar {{
        align-items: flex-start;
        flex-direction: column;
        padding: 14px 16px;
      }}

      .canvas-pane {{
        min-height: 68vh;
        padding: 14px;
      }}

      .console-pane {{
        border-left: 0;
        border-top: 1px solid var(--line);
      }}

      h1 {{
        white-space: normal;
      }}
    }}
  </style>
</head>
<body>
  <div class="artifact-shell" data-t2w-artifact-shell="default" data-design-system="T2W Industrial Studio">
    <nav class="rail" aria-label="Artifact shell">
      <div class="brand-dot">t</div>
      <div class="rail-mark is-active" title="Artifacts">▱</div>
      <div class="rail-mark" title="Documents">▤</div>
      <div class="rail-mark" title="Data">◎</div>
      <div class="rail-mark" title="Theme">◌</div>
      <div class="rail-mark" title="History">↺</div>
    </nav>

    <header class="topbar">
      <div class="topbar-title">
        <span class="product">t2w</span>
        <span class="divider" aria-hidden="true"></span>
        <span class="artifact-name">{template_name}</span>
      </div>
      <div class="topbar-actions">
        <div class="status-strip">
          <span class="status-pill" id="stream-status">ready</span>
          <span id="byte-count">0 bytes</span>
        </div>
        <button id="export-html" class="action-button" type="button">Export HTML</button>
        <button id="run-generate" class="action-button primary" type="button">Run/Generate</button>
        <button class="action-button" type="button" aria-label="Graph view">⌘</button>
        <button class="action-button" type="button" aria-label="Settings">⚙</button>
      </div>
    </header>

    <main class="canvas-pane">
      <div class="canvas-header">
        <div>
          <p class="eyebrow">Generated artifact</p>
          <h1>{title}</h1>
          <p class="subtle">{template_description}</p>
        </div>
        <div class="canvas-tools">
          <button id="toggle-fullscreen" class="action-button" type="button" aria-label="Toggle fullscreen preview">⛶</button>
        </div>
      </div>

      <div class="preview-frame-wrap">
        <div class="preview-label">Preview Canvas</div>
        <iframe id="artifact-frame" title="Generated artifact preview" sandbox="allow-scripts" srcdoc="{escaped_artifact_html}"></iframe>
      </div>
    </main>

    <aside class="console-pane" aria-label="Artifact console">
      <div class="console-head">
        <h2>Configuration</h2>
        <p class="subtle">Utility Panel</p>
      </div>

      <div class="tabs" role="tablist" aria-label="Artifact console tabs">
        <button id="tab-prompt" class="tab" type="button" role="tab" aria-selected="true" data-tab="prompt"><span>▣</span><span>Prompt</span></button>
        <button id="tab-data" class="tab" type="button" role="tab" aria-selected="false" data-tab="data"><span>▦</span><span>Data</span></button>
        <button id="tab-live" class="tab" type="button" role="tab" aria-selected="false" data-tab="live"><span>▤</span><span>Live</span></button>
        <button id="tab-theme" class="tab" type="button" role="tab" aria-selected="false" data-tab="theme"><span>▥</span><span>Theme</span></button>
        <button id="tab-run" class="tab" type="button" role="tab" aria-selected="false" data-tab="run"><span>▷</span><span>Run</span></button>
      </div>

      <section class="panel is-active" role="tabpanel" data-panel="prompt">
        <p class="eyebrow">System Prompt</p>
        <div class="readout"><pre>You are t2w, a terminal-to-web artifact engine. Return self-contained HTML for the preview canvas.</pre></div>
        <p class="eyebrow">User Instruction</p>
        <div class="readout"><pre>{instruction}</pre></div>
        <div class="actions">
          <button id="copy-instruction" class="action-button" type="button">Copy instruction</button>
        </div>
      </section>

      <section class="panel" role="tabpanel" data-panel="data">
        <p class="eyebrow">Input Data</p>
        <div class="readout"><pre>{stdin}</pre></div>
      </section>

      <section class="panel" role="tabpanel" data-panel="live">
        <p class="eyebrow">Live Render</p>
        <div class="console-live-frame-wrap">
          <iframe id="console-live-frame" title="Console live artifact render" sandbox="allow-scripts" srcdoc="{escaped_artifact_html}"></iframe>
        </div>
      </section>

      <section class="panel" role="tabpanel" data-panel="theme">
        <p class="eyebrow">Template</p>
        <div class="meta-list">
          <div class="meta-row"><span>Name</span><strong>{template_name}</strong></div>
          <div class="meta-row"><span>Surface</span><strong>Industrial Studio</strong></div>
          <div class="meta-row"><span>Accent</span><strong>Amber #f59e0b</strong></div>
        </div>
        <div class="readout"><pre>{template_description}</pre></div>
        <p class="eyebrow">Reinforcement Formula</p>
        <div class="formula-list">{formula}</div>
      </section>

      <section class="panel" role="tabpanel" data-panel="run">
        <p class="eyebrow">Execution</p>
        <div class="meta-list">
          <div class="meta-row"><span>Provider</span><strong>{provider}</strong></div>
          <div class="meta-row"><span>Model</span><strong>{model}</strong></div>
          <div class="meta-row"><span>Template</span><strong>{template_name}</strong></div>
          <div class="meta-row"><span>Status</span><strong id="run-status-detail">ready</strong></div>
          <div class="meta-row"><span>Snapshot</span><strong>local artifact</strong></div>
        </div>
        <div class="actions">
          <button id="download-inner" class="action-button primary" type="button">Download inner HTML</button>
        </div>
      </section>
    </aside>
  </div>

  <script id="t2w-run-context" type="application/json">{metadata_json}</script>
  <script>
    (function () {{
      const frame = document.getElementById("artifact-frame");
      const consoleFrame = document.getElementById("console-live-frame");
      const status = document.getElementById("stream-status");
      const statusDetail = document.getElementById("run-status-detail");
      const byteCount = document.getElementById("byte-count");
      const context = JSON.parse(document.getElementById("t2w-run-context").textContent || "{{}}");
      const chunks = [];
      const scheduleFrame = window.requestAnimationFrame
        ? window.requestAnimationFrame.bind(window)
        : function (callback) {{ return window.setTimeout(callback, 16); }};
      let previewRenderQueued = false;

      function currentHtml() {{
        return chunks.length ? chunks.join("") : (frame.getAttribute("srcdoc") || "");
      }}

      function setRunStatus(value) {{
        status.textContent = value;
        if (statusDetail) {{
          statusDetail.textContent = value;
        }}
        document.body.dataset.runStatus = value;
      }}

      function updateBytes(value) {{
        byteCount.textContent = new Blob([value]).size + " bytes";
      }}

      function downloadInnerHtml() {{
        const blob = new Blob([currentHtml()], {{ type: "text/html;charset=utf-8" }});
        const url = URL.createObjectURL(blob);
        const link = document.createElement("a");
        link.href = url;
        link.download = (context.title || "artifact").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") + "-inner.html";
        document.body.appendChild(link);
        link.click();
        link.remove();
        URL.revokeObjectURL(url);
      }}

      function simulateRun() {{
        setRunStatus("ready");
        window.setTimeout(function () {{
          setRunStatus("streaming");
        }}, 120);
        window.setTimeout(function () {{
          setRunStatus("completed");
          updateBytes(currentHtml());
        }}, 700);
      }}

      function renderConsoleNow(html) {{
        if (consoleFrame) {{
          consoleFrame.srcdoc = html;
        }}
      }}

      function renderPreviewNow() {{
        previewRenderQueued = false;
        const html = currentHtml();
        frame.srcdoc = html;
        renderConsoleNow(html);
        updateBytes(html);
      }}

      function schedulePreviewRender() {{
        if (previewRenderQueued) {{
          return;
        }}
        previewRenderQueued = true;
        scheduleFrame(renderPreviewNow);
      }}

      window.__t2wAppendArtifactChunk = function (chunk) {{
        chunks.push(chunk);
        setRunStatus("streaming");
        schedulePreviewRender();
      }};

      window.__t2wMarkArtifactComplete = function () {{
        renderPreviewNow();
        setRunStatus("completed");
        updateBytes(currentHtml());
      }};

      document.querySelectorAll(".tab").forEach(function (tab) {{
        tab.addEventListener("click", function () {{
          document.querySelectorAll(".tab").forEach(function (item) {{
            item.setAttribute("aria-selected", String(item === tab));
          }});
          document.querySelectorAll(".panel").forEach(function (panel) {{
            panel.classList.toggle("is-active", panel.dataset.panel === tab.dataset.tab);
          }});
        }});
      }});

      document.getElementById("copy-instruction").addEventListener("click", async function () {{
        try {{
          await navigator.clipboard.writeText(context.instruction || "");
          this.textContent = "Instruction copied";
        }} catch (_error) {{
          this.textContent = "Copy unavailable";
        }}
      }});

      document.getElementById("toggle-fullscreen").addEventListener("click", async function () {{
        const target = document.querySelector(".preview-frame-wrap");
        try {{
          if (!document.fullscreenElement) {{
            await target.requestFullscreen();
            this.textContent = "⛶";
          }} else {{
            await document.exitFullscreen();
            this.textContent = "⛶";
          }}
        }} catch (_error) {{
          this.textContent = "!";
        }}
      }});

      document.getElementById("download-inner").addEventListener("click", downloadInnerHtml);
      document.getElementById("export-html").addEventListener("click", downloadInnerHtml);
      document.getElementById("run-generate").addEventListener("click", simulateRun);

      setRunStatus("ready");
      updateBytes(currentHtml());
    }}());
  </script>
  {extra_script}
</body>
</html>"#
    )
}

fn shell_metadata_json(context: &ArtifactShellContext) -> String {
    let metadata = ArtifactShellMetadata {
        shell: "default",
        title: &context.title,
        instruction: &context.instruction,
        stdin: &context.stdin,
        template_name: &context.template_name,
        template_description: &context.template_description,
        provider: &context.provider,
        model: &context.model,
        formula: &context.formula,
    };

    safe_script_json(&serde_json::to_string(&metadata).unwrap_or_else(|_| "{}".to_string()))
}

fn render_formula(stages: &[FormulaStep]) -> String {
    stages
        .iter()
        .map(|stage| {
            let id = escape_html(&stage.id.to_ascii_uppercase());
            let title = escape_html(&stage.title);
            let description = escape_html(&stage.description);
            let rules = stage
                .rules
                .iter()
                .map(|rule| format!("<li>{}</li>", escape_html(rule)))
                .collect::<Vec<_>>()
                .join("");
            format!(
                r#"<article class="formula-stage"><div class="formula-stage-head"><span>{id}</span><strong>{title}</strong></div><p>{description}</p><ul>{rules}</ul></article>"#
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_pre_text(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        escape_html(fallback)
    } else {
        escape_html(value)
    }
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn safe_script_json(json: &str) -> String {
    json.replace("</", "<\\/")
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactShellContext, artifact_title_from_instruction, render_artifact_shell,
        render_artifact_shell_stream_chunk, render_artifact_shell_stream_start,
    };

    fn context() -> ArtifactShellContext {
        ArtifactShellContext::new(
            "Production Cluster Logs",
            "Filter latest entries",
            "GET /health 500",
            "Artifact Console",
            "Default CLI control surface",
            "mock",
            "mock-model",
        )
    }

    #[test]
    fn render_artifact_shell_wraps_inner_html_with_console_context() {
        let shell = render_artifact_shell(&context(), "<main><h1>Mock Artifact</h1></main>");

        assert!(shell.starts_with("<!DOCTYPE html>"));
        assert!(shell.contains("data-t2w-artifact-shell=\"default\""));
        assert!(shell.contains("T2W Industrial Studio"));
        assert!(shell.contains("User Instruction"));
        assert!(shell.contains("Filter latest entries"));
        assert!(shell.contains("GET /health 500"));
        assert!(shell.contains("&lt;main&gt;&lt;h1&gt;Mock Artifact&lt;/h1&gt;&lt;/main&gt;"));
    }

    #[test]
    fn render_artifact_shell_exposes_studio_interaction_controls() {
        let shell = render_artifact_shell(&context(), "<main><h1>Mock Artifact</h1></main>");

        assert!(shell.contains("id=\"tab-prompt\""));
        assert!(shell.contains("id=\"tab-data\""));
        assert!(shell.contains("id=\"tab-live\""));
        assert!(shell.contains("id=\"tab-theme\""));
        assert!(shell.contains("id=\"tab-run\""));
        assert!(shell.contains("id=\"console-live-frame\""));
        assert!(shell.contains("data-panel=\"live\""));
        assert!(shell.contains("data-panel=\"theme\""));
        assert!(shell.contains("id=\"export-html\""));
        assert!(shell.contains("id=\"run-generate\""));
        assert!(shell.contains("id=\"toggle-fullscreen\""));
        assert!(shell.contains("simulateRun"));
        assert!(shell.contains("setRunStatus(\"streaming\")"));
        assert!(shell.contains("renderConsoleNow"));
        assert!(shell.contains("downloadInnerHtml"));
    }

    #[test]
    fn render_artifact_shell_exposes_reinforcement_formula() {
        let shell = render_artifact_shell(&context(), "<main><h1>Mock Artifact</h1></main>");

        assert!(shell.contains("Reinforcement Formula"));
        assert!(shell.contains("Prompt Formula"));
        assert!(shell.contains("Data Formula"));
        assert!(shell.contains("Theme Formula"));
        assert!(shell.contains("Run Formula"));
    }

    #[test]
    fn render_stream_chunk_escapes_script_closers() {
        let chunk = render_artifact_shell_stream_chunk("<script></script><h1>ok</h1>");

        assert!(chunk.contains("<\\/script>"));
        assert!(!chunk.contains("</script><h1>ok"));
    }

    #[test]
    fn render_stream_start_schedules_live_preview_refreshes_on_animation_frames() {
        let shell = render_artifact_shell_stream_start(&context());

        assert!(shell.contains("requestAnimationFrame"));
        assert!(shell.contains("schedulePreviewRender"));
    }

    #[test]
    fn artifact_title_from_instruction_uses_readable_fallback_and_limit() {
        assert_eq!(
            artifact_title_from_instruction(""),
            "Untitled artifact".to_string()
        );
        assert_eq!(
            artifact_title_from_instruction("build a dashboard from logs"),
            "build a dashboard from logs".to_string()
        );
    }
}
