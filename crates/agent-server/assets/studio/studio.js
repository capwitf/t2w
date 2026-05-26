(function () {
  const app = document.getElementById("studio-app");

  const elements = {
    appHeader: document.querySelector(".app-header"),
    configToggle: document.getElementById("config-toggle"),
    globalMessage: document.getElementById("global-message"),
    newDraftButton: document.getElementById("new-draft-button"),
    sessionList: document.getElementById("session-list"),
    draftBanner: document.getElementById("draft-banner"),
    sessionTitleHeading: document.getElementById("session-title-heading"),
    sessionStatusBadge: document.getElementById("session-status-badge"),
    runStatusBadge: document.getElementById("run-status-badge"),
    runButton: document.getElementById("run-button"),
    cancelButton: document.getElementById("cancel-button"),
    downloadButton: document.getElementById("download-button"),
    selectedTemplateName: document.getElementById("selected-template-name"),
    selectedTemplateDescription: document.getElementById("selected-template-description"),
    providerSummary: document.getElementById("provider-summary"),
    modelSummary: document.getElementById("model-summary"),
    snapshotSummary: document.getElementById("snapshot-summary"),
    previewFrame: document.getElementById("preview-frame"),
    previewPlaceholder: document.getElementById("preview-placeholder"),
    liveMiniFrame: document.getElementById("live-mini-frame"),
    htmlSourceView: document.getElementById("html-source-view"),
    copyHtmlButton: document.getElementById("copy-html-button"),
    runPhase: document.getElementById("run-phase"),
    runSnapshot: document.getElementById("run-snapshot"),
    runError: document.getElementById("run-error"),
    runStatusDetail: document.getElementById("run-status-detail"),
    runPhaseDetail: document.getElementById("run-phase-detail"),
    runSnapshotDetail: document.getElementById("run-snapshot-detail"),
    runErrorDetail: document.getElementById("run-error-detail"),
    titleInput: document.getElementById("title-input"),
    instructionInput: document.getElementById("instruction-input"),
    stdinInput: document.getElementById("stdin-input"),
    templateSelect: document.getElementById("template-select"),
    templateDescription: document.getElementById("template-description"),
    providerSelect: document.getElementById("provider-select"),
    modelInput: document.getElementById("model-input"),
    persistSnapshotInput: document.getElementById("persist-snapshot-input"),
    formulaList: document.getElementById("formula-list"),
    skillsList: document.getElementById("skills-list"),
    tabButtons: Array.from(document.querySelectorAll(".tab-button")),
    tabPanels: Array.from(document.querySelectorAll(".tab-panel")),
  };

  const api = {
    sessions: app.dataset.sessionsUrl || "/sessions",
    templates: app.dataset.templatesUrl || "/templates",
    skills: app.dataset.skillsUrl || "/skills",
  };

  const mobileQuery = window.matchMedia("(max-width: 980px)");

  const state = {
    templates: [],
    skills: [],
    sessions: [],
    form: createDraft(),
    selectedSessionId: null,
    currentRun: null,
    activeTab: "prompt",
    panelOpen: !mobileQuery.matches,
    message: null,
    eventSource: null,
    busy: false,
    rawHtml: "",
    artifactStreamComplete: false,
    streamAbort: null,
    streamGeneration: 0,
    previewScheduled: false,
  };

  init();

  function init() {
    applyPanelMode();
    bindEvents();
    renderStaticSelections();
    render();
    loadBootstrap();
  }

  function bindEvents() {
    elements.configToggle.addEventListener("click", function () {
      state.panelOpen = !state.panelOpen;
      render();
    });

    mobileQuery.addEventListener("change", function () {
      if (!mobileQuery.matches) {
        state.panelOpen = true;
      } else {
        state.panelOpen = false;
      }
      render();
    });

    elements.newDraftButton.addEventListener("click", function () {
      disconnectRunStream();
      abortArtifactStream();
      state.selectedSessionId = null;
      state.currentRun = null;
      state.rawHtml = "";
      state.artifactStreamComplete = false;
      state.form = createDraft();
      clearMessage();
      renderPreview();
      render();
    });

    elements.runButton.addEventListener("click", function () {
      void startRun();
    });

    elements.cancelButton.addEventListener("click", function () {
      void cancelRun();
    });

    elements.downloadButton.addEventListener("click", function () {
      void downloadArtifact();
    });

    elements.copyHtmlButton.addEventListener("click", function () {
      if (state.rawHtml) {
        navigator.clipboard.writeText(state.rawHtml);
        setMessage("info", "HTML copied to clipboard.");
        render();
      }
    });

    elements.titleInput.addEventListener("input", function (event) {
      state.form.title = event.target.value;
      render();
    });

    elements.instructionInput.addEventListener("input", function (event) {
      state.form.instruction = event.target.value;
      render();
    });

    elements.stdinInput.addEventListener("input", function (event) {
      state.form.input.stdin = event.target.value;
    });

    elements.templateSelect.addEventListener("change", function (event) {
      state.form.template_id = event.target.value;
      render();
    });

    elements.providerSelect.addEventListener("change", function (event) {
      state.form.provider = event.target.value;
      render();
    });

    elements.modelInput.addEventListener("input", function (event) {
      state.form.model = event.target.value;
      render();
    });

    elements.persistSnapshotInput.addEventListener("change", function (event) {
      state.form.options.persist_snapshot = Boolean(event.target.checked);
      render();
    });

    elements.tabButtons.forEach(function (button) {
      button.addEventListener("click", function () {
        state.activeTab = button.dataset.tab;
        render();
      });
    });
  }

  async function loadBootstrap() {
    setMessage("info", "Loading Studio state...");
    try {
      const results = await Promise.all([
        fetchJson(api.templates),
        fetchJson(api.skills),
        fetchJson(api.sessions),
      ]);
      state.templates = results[0].templates || [];
      state.skills = results[1].skills || [];
      state.sessions = sortSessions(results[2].sessions || []);

      if (state.templates.length && !state.templates.some(function (template) { return template.id === state.form.template_id; })) {
        state.form.template_id = state.templates[0].id;
      }

      renderStaticSelections();

      if (state.sessions.length > 0) {
        await selectSession(state.sessions[0].id);
        clearMessage();
      } else {
        state.form = createDraft();
        clearMessage();
        render();
      }
    } catch (error) {
      setMessage("error", error.message);
      render();
    }
  }

  async function selectSession(sessionId) {
    const session = state.sessions.find(function (item) {
      return item.id === sessionId;
    });
    if (!session) {
      return;
    }

    disconnectRunStream();
    abortArtifactStream();
    state.selectedSessionId = session.id;
    state.form = cloneSession(session);
    state.currentRun = null;
    state.rawHtml = "";
    state.artifactStreamComplete = false;
    renderStaticSelections();
    renderPreview();

    if (session.latest_run_id) {
      try {
        const run = await fetchJson("/runs/" + session.latest_run_id);
        connectRun(run);
      } catch (error) {
        setMessage("error", error.message);
      }
    }

    if (mobileQuery.matches) {
      state.panelOpen = false;
    }

    render();
  }

  async function refreshCurrentSession() {
    if (!state.selectedSessionId) {
      return;
    }
    const session = await fetchJson(api.sessions + "/" + state.selectedSessionId);
    upsertSession(session);
    state.form = cloneSession(session);
  }

  async function ensureSessionSaved() {
    const payload = serializeForm(state.form);
    if (!state.selectedSessionId) {
      const created = await fetchJson(api.sessions, {
        method: "POST",
        body: JSON.stringify(payload),
      });
      state.selectedSessionId = created.id;
      upsertSession(created);
      state.form = cloneSession(created);
      return created;
    }

    const updated = await fetchJson(api.sessions + "/" + state.selectedSessionId, {
      method: "PATCH",
      body: JSON.stringify(payload),
    });
    upsertSession(updated);
    state.form = cloneSession(updated);
    return updated;
  }

  async function startRun() {
    if (runIsActive(state.currentRun) || state.busy) {
      return;
    }

    state.busy = true;
    clearMessage();
    render();

    try {
      const session = await ensureSessionSaved();
      const run = await fetchJson(api.sessions + "/" + session.id + "/runs", {
        method: "POST",
      });
      connectRun(run);
      upsertSession({
        id: session.id,
        title: state.form.title,
        status: "active",
        instruction: state.form.instruction,
        input: { stdin: state.form.input.stdin },
        template_id: state.form.template_id,
        skill_ids: state.form.skill_ids.slice(),
        provider: state.form.provider,
        model: state.form.model,
        options: { persist_snapshot: state.form.options.persist_snapshot },
        latest_run_id: run.id,
        active_run_id: run.id,
        created_at: session.created_at,
        updated_at: new Date().toISOString(),
      });
      setMessage("info", "Run started.");
    } catch (error) {
      setMessage("error", error.message);
    } finally {
      state.busy = false;
      render();
    }
  }

  async function cancelRun() {
    if (!state.selectedSessionId || !state.currentRun || state.busy) {
      return;
    }

    state.busy = true;
    abortArtifactStream();
    render();

    try {
      const canceled = await fetchJson(
        api.sessions + "/" + state.selectedSessionId + "/runs/" + state.currentRun.id + "/cancel",
        { method: "POST" }
      );
      connectRun(canceled);
      await refreshCurrentSession();
      setMessage("info", "Run canceled.");
    } catch (error) {
      setMessage("error", error.message);
    } finally {
      state.busy = false;
      render();
    }
  }

  async function downloadArtifact() {
    if (!canDownload()) {
      return;
    }

    try {
      const html = state.rawHtml;
      if (!html) {
        setMessage("error", "No HTML content available to download.");
        render();
        return;
      }
      const blob = new Blob([html], { type: "text/html;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = buildDownloadName();
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
      setMessage("info", "Downloaded current artifact.");
    } catch (error) {
      setMessage("error", error.message);
    } finally {
      render();
    }
  }

  function connectRun(run) {
    disconnectRunStream();
    abortArtifactStream();
    state.currentRun = run;
    state.rawHtml = "";
    state.artifactStreamComplete = false;

    if (run.artifact_url) {
      streamArtifact(run.artifact_url);
    }

    if (runIsActive(run)) {
      state.eventSource = new EventSource(run.events_url);
      state.eventSource.addEventListener("run", function (event) {
        const nextRun = JSON.parse(event.data);
        state.currentRun = nextRun;
        synchronizeSessionFromRun(nextRun);
        render();

        if (!runIsActive(nextRun)) {
          disconnectRunStream();
          void refreshCurrentSession().then(function () {
            render();
          }).catch(function (error) {
            setMessage("error", error.message);
            render();
          });
        }
      });
      state.eventSource.onerror = function () {
        disconnectRunStream();
        if (runIsActive(state.currentRun)) {
          setMessage("error", "The run event stream disconnected before completion.");
          render();
        }
      };
    } else {
      synchronizeSessionFromRun(run);
    }
  }

  function abortArtifactStream() {
    if (state.streamAbort) {
      state.streamAbort.abort();
      state.streamAbort = null;
    }
    state.artifactStreamComplete = false;
    state.streamGeneration += 1;
  }

  async function streamArtifact(url) {
    const controller = new AbortController();
    state.streamAbort = controller;
    const generation = state.streamGeneration;
    renderPreview();

    let reader = null;
    try {
      const response = await fetch(url, {
        credentials: "same-origin",
        signal: controller.signal,
      });
      if (!response.ok || generation !== state.streamGeneration) {
        if (generation === state.streamGeneration) state.streamAbort = null;
        return;
      }
      reader = response.body.getReader();
      const decoder = new TextDecoder();

      while (true) {
        const result = await reader.read();
        if (result.done) break;
        if (generation !== state.streamGeneration) {
          reader.cancel();
          return;
        }
        state.rawHtml += decoder.decode(result.value, { stream: true });
        schedulePreviewUpdate();
      }
      if (generation !== state.streamGeneration) return;
      state.rawHtml += decoder.decode();
      state.artifactStreamComplete = true;
      state.streamAbort = null;
      renderPreview();
      render();
    } catch (error) {
      if (generation !== state.streamGeneration) return;
      state.streamAbort = null;
      if (error.name !== "AbortError") {
        setMessage("error", "Artifact stream failed: " + error.message);
        render();
      }
    }
  }

  function schedulePreviewUpdate() {
    if (state.previewScheduled) return;
    state.previewScheduled = true;
    requestAnimationFrame(function () {
      state.previewScheduled = false;
      renderPreview();
    });
  }

  function renderPreview() {
    const html = state.rawHtml;
    elements.previewPlaceholder.hidden = Boolean(html);
    if (html) {
      elements.previewFrame.srcdoc = html;
      if (elements.liveMiniFrame) {
        elements.liveMiniFrame.srcdoc = html;
      }
    } else {
      elements.previewFrame.srcdoc = "";
      if (elements.liveMiniFrame) {
        elements.liveMiniFrame.srcdoc = "";
      }
    }
    if (elements.htmlSourceView) {
      elements.htmlSourceView.textContent = html;
    }
  }

  function synchronizeSessionFromRun(run) {
    if (!state.selectedSessionId) {
      return;
    }
    const session = state.sessions.find(function (item) {
      return item.id === state.selectedSessionId;
    });
    if (!session) {
      return;
    }
    session.latest_run_id = run.id;
    session.active_run_id = runIsActive(run) ? run.id : null;
    session.status = runIsActive(run) ? "active" : mapRunStatusToSessionStatus(run.status);
    session.updated_at = run.finished_at || run.started_at || run.created_at;
  }

  function disconnectRunStream() {
    if (state.eventSource) {
      state.eventSource.close();
      state.eventSource = null;
    }
  }

  function upsertSession(session) {
    const normalized = cloneSession(session);
    const existingIndex = state.sessions.findIndex(function (item) {
      return item.id === normalized.id;
    });
    if (existingIndex >= 0) {
      state.sessions.splice(existingIndex, 1, normalized);
    } else {
      state.sessions.push(normalized);
    }
    state.sessions = sortSessions(state.sessions);
  }

  function render() {
    applyPanelMode();
    updateHeaderHeight();
    renderMessage();
    renderTabs();
    renderSessionList();
    renderForm();
    renderRunState();
  }

  function renderStaticSelections() {
    elements.templateSelect.innerHTML = "";
    state.templates.forEach(function (template) {
      const option = document.createElement("option");
      option.value = template.id;
      option.textContent = template.name;
      elements.templateSelect.appendChild(option);
    });

    elements.skillsList.innerHTML = "";
    state.skills.forEach(function (skill) {
      const label = document.createElement("label");
      label.className = "option-row";

      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.value = skill.name;
      checkbox.addEventListener("change", function (event) {
        if (event.target.checked) {
          if (!state.form.skill_ids.includes(skill.name)) {
            state.form.skill_ids.push(skill.name);
          }
        } else {
          state.form.skill_ids = state.form.skill_ids.filter(function (id) {
            return id !== skill.name;
          });
        }
        render();
      });

      const copy = document.createElement("span");
      copy.className = "option-copy";
      copy.innerHTML = "<strong>" + escapeHtml(skill.name) + "</strong><span>" + escapeHtml(skill.description) + "</span>";

      label.appendChild(checkbox);
      label.appendChild(copy);
      elements.skillsList.appendChild(label);
    });
  }

  function renderMessage() {
    if (!state.message) {
      elements.globalMessage.hidden = true;
      elements.globalMessage.removeAttribute("data-tone");
      elements.globalMessage.textContent = "";
      return;
    }
    elements.globalMessage.hidden = false;
    elements.globalMessage.dataset.tone = state.message.tone;
    elements.globalMessage.textContent = state.message.text;
  }

  function renderTabs() {
    elements.tabButtons.forEach(function (button) {
      const active = button.dataset.tab === state.activeTab;
      button.classList.toggle("is-active", active);
      button.setAttribute("aria-selected", active ? "true" : "false");
    });

    elements.tabPanels.forEach(function (panel) {
      panel.classList.toggle("is-active", panel.dataset.panel === state.activeTab);
    });
  }

  function renderSessionList() {
    elements.sessionList.innerHTML = "";

    const draftButton = document.createElement("button");
    draftButton.type = "button";
    draftButton.className = "session-item" + (state.selectedSessionId ? "" : " is-selected");
    draftButton.innerHTML =
      "<span class=\"session-item-title\">Local draft</span>" +
      "<span class=\"session-item-meta\">Unsaved changes remain in this browser until you run.</span>";
    draftButton.addEventListener("click", function () {
      disconnectRunStream();
      abortArtifactStream();
      state.selectedSessionId = null;
      state.currentRun = null;
      state.rawHtml = "";
      state.artifactStreamComplete = false;
      state.form = createDraft();
      clearMessage();
      renderPreview();
      render();
    });
    elements.sessionList.appendChild(draftButton);

    state.sessions.forEach(function (session) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "session-item" + (session.id === state.selectedSessionId ? " is-selected" : "");
      button.innerHTML =
        "<span class=\"session-item-title\">" + escapeHtml(session.title || "Untitled session") + "</span>" +
        "<span class=\"session-item-meta\">" + escapeHtml(session.status) + " · " + escapeHtml(formatTimestamp(session.updated_at)) + "</span>";
      button.addEventListener("click", function () {
        void selectSession(session.id);
      });
      elements.sessionList.appendChild(button);
    });

    elements.draftBanner.textContent = state.selectedSessionId
      ? "Saved session loaded from the current t2w-studio process."
      : "No saved session selected. Changes stay local until you run.";
  }

  function renderForm() {
    const template = currentTemplate();
    const run = state.currentRun;

    elements.sessionTitleHeading.textContent = state.form.title || "Local draft";
    elements.sessionStatusBadge.textContent = state.selectedSessionId ? state.form.status : "draft";
    elements.titleInput.value = state.form.title;
    elements.instructionInput.value = state.form.instruction;
    elements.stdinInput.value = state.form.input.stdin;
    elements.templateSelect.value = state.form.template_id;
    elements.templateDescription.textContent = template ? template.description : "No template available.";
    elements.selectedTemplateName.textContent = template ? template.name : state.form.template_id;
    elements.selectedTemplateDescription.textContent = template ? template.description : "No template description available.";
    renderFormulaList(template ? template.formula : []);
    elements.providerSelect.value = state.form.provider;
    elements.modelInput.value = state.form.model;
    elements.persistSnapshotInput.checked = Boolean(state.form.options.persist_snapshot);
    elements.providerSummary.textContent = state.form.provider;
    elements.modelSummary.textContent = state.form.model;
    elements.snapshotSummary.textContent = state.form.options.persist_snapshot ? "enabled" : "disabled";

    Array.from(elements.skillsList.querySelectorAll("input[type='checkbox']")).forEach(function (checkbox) {
      checkbox.checked = state.form.skill_ids.includes(checkbox.value);
    });
  }

  function renderRunState() {
    const run = state.currentRun;
    const active = runIsActive(run);
    const busy = state.busy;

    elements.runButton.disabled = active || busy;
    elements.runButton.textContent = active ? "Running..." : "Run";
    elements.cancelButton.hidden = !active;
    elements.cancelButton.disabled = !active || busy;
    elements.downloadButton.disabled = !canDownload();
    elements.runStatusBadge.textContent = run ? run.status : "idle";
    elements.runStatusBadge.classList.toggle("muted", !run || run.status === "idle");
    elements.runPhase.textContent = run ? run.phase : "idle";
    elements.runSnapshot.textContent = run && run.snapshot_path ? run.snapshot_path : "none";
    elements.runError.textContent = run && run.error ? run.error : "none";
    elements.runStatusDetail.textContent = run ? run.status : "idle";
    elements.runPhaseDetail.textContent = run ? run.phase : "idle";
    elements.runSnapshotDetail.textContent = run && run.snapshot_path ? run.snapshot_path : "none";
    elements.runErrorDetail.textContent = run && run.error ? run.error : "none";
  }

  function renderFormulaList(formula) {
    elements.formulaList.innerHTML = "";
    const stages = Array.isArray(formula) ? formula : [];
    if (!stages.length) {
      const empty = document.createElement("p");
      empty.className = "subtle-copy";
      empty.textContent = "No reinforcement formula is available for this template.";
      elements.formulaList.appendChild(empty);
      return;
    }

    stages.forEach(function (stage) {
      const section = document.createElement("section");
      section.className = "formula-step";

      const head = document.createElement("div");
      head.className = "formula-step-head";

      const code = document.createElement("span");
      code.textContent = String(stage.id || "").toUpperCase();

      const title = document.createElement("strong");
      title.textContent = stage.title || "Formula";

      head.appendChild(code);
      head.appendChild(title);
      section.appendChild(head);

      const description = document.createElement("p");
      description.textContent = stage.description || "";
      section.appendChild(description);

      const rules = document.createElement("ul");
      (Array.isArray(stage.rules) ? stage.rules : []).forEach(function (rule) {
        const item = document.createElement("li");
        item.textContent = rule;
        rules.appendChild(item);
      });
      section.appendChild(rules);
      elements.formulaList.appendChild(section);
    });
  }

  function applyPanelMode() {
    document.body.dataset.panelOpen = state.panelOpen ? "true" : "false";
    elements.configToggle.setAttribute("aria-expanded", state.panelOpen ? "true" : "false");
  }

  function updateHeaderHeight() {
    if (!elements.appHeader) {
      return;
    }
    document.documentElement.style.setProperty("--header-height", elements.appHeader.offsetHeight + "px");
  }

  function canDownload() {
    return Boolean(
      state.rawHtml &&
      state.artifactStreamComplete &&
      state.currentRun &&
      !runIsActive(state.currentRun)
    );
  }

  function currentTemplate() {
    return state.templates.find(function (template) {
      return template.id === state.form.template_id;
    }) || null;
  }

  function createDraft() {
    return {
      id: null,
      title: "Local draft",
      status: "draft",
      instruction: "",
      input: { stdin: "" },
      template_id: "table-explorer",
      skill_ids: [],
      provider: "mock",
      model: "mock-model",
      options: { persist_snapshot: true },
      latest_run_id: null,
      active_run_id: null,
      created_at: null,
      updated_at: null,
    };
  }

  function cloneSession(session) {
    return {
      id: session.id || null,
      title: session.title || "Untitled session",
      status: session.status || "draft",
      instruction: session.instruction || "",
      input: { stdin: (session.input && session.input.stdin) || "" },
      template_id: session.template_id || "table-explorer",
      skill_ids: Array.isArray(session.skill_ids) ? session.skill_ids.slice() : [],
      provider: session.provider || "mock",
      model: session.model || "mock-model",
      options: {
        persist_snapshot: Boolean(session.options && session.options.persist_snapshot),
      },
      latest_run_id: session.latest_run_id || null,
      active_run_id: session.active_run_id || null,
      created_at: session.created_at || null,
      updated_at: session.updated_at || null,
    };
  }

  function serializeForm(form) {
    return {
      title: (form.title || "").trim() || "Untitled session",
      instruction: form.instruction,
      input: { stdin: form.input.stdin },
      template_id: form.template_id,
      skill_ids: form.skill_ids.slice(),
      provider: form.provider,
      model: form.model,
      options: { persist_snapshot: Boolean(form.options.persist_snapshot) },
    };
  }

  function sortSessions(sessions) {
    return sessions.slice().sort(function (left, right) {
      return String(right.updated_at || "").localeCompare(String(left.updated_at || ""));
    });
  }

  function runIsActive(run) {
    return Boolean(run && run.status === "running");
  }

  function mapRunStatusToSessionStatus(status) {
    if (status === "completed") {
      return "completed";
    }
    if (status === "failed") {
      return "failed";
    }
    if (status === "canceled") {
      return "canceled";
    }
    return "active";
  }

  function setMessage(tone, text) {
    state.message = { tone: tone, text: text };
  }

  function clearMessage() {
    state.message = null;
  }

  async function fetchJson(url, options) {
    const response = await fetch(url, {
      method: options && options.method ? options.method : "GET",
      headers: {
        "content-type": "application/json",
      },
      credentials: "same-origin",
      body: options && options.body ? options.body : undefined,
    });

    if (response.ok) {
      return response.json();
    }

    let message = "Request failed.";
    try {
      const payload = await response.json();
      if (payload && payload.message) {
        message = payload.message;
        if (payload.error) {
          message += " (" + payload.error + ")";
        }
      }
    } catch (error) {
      const text = await response.text();
      if (text) {
        message = text;
      }
    }

    throw new Error(message);
  }

  function formatTimestamp(value) {
    if (!value) {
      return "not saved";
    }
    return value.replace("T", " ").replace("Z", " UTC");
  }

  function buildDownloadName() {
    const title = (state.form.title || "artifact").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
    const runId = state.currentRun ? state.currentRun.id : "latest";
    return (title || "artifact") + "-" + runId + ".html";
  }

  function escapeHtml(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }
})();
