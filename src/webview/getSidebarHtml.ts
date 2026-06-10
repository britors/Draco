const MONACO_VERSION = '0.45.0';
const MONACO_CDN = `https://cdn.jsdelivr.net/npm/monaco-editor@${MONACO_VERSION}/min/vs`;

export function getSidebarHtml(params: {
  nonce: string;
  cspSource: string;
  codiconsUri: string;
}): string {
  const { nonce, cspSource, codiconsUri } = params;

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy"
    content="default-src 'none';
             script-src 'nonce-${nonce}' https://cdn.jsdelivr.net;
             style-src ${cspSource} 'unsafe-inline' https://cdn.jsdelivr.net;
             font-src ${cspSource} https://cdn.jsdelivr.net data:;
             worker-src blob:;
             connect-src https://cdn.jsdelivr.net;">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <link rel="stylesheet" href="${codiconsUri}">
  <title>Draco</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
      color: var(--vscode-foreground);
      background: var(--vscode-sideBar-background);
      height: 100vh;
      overflow: hidden;
      display: flex;
      flex-direction: column;
    }
    .hidden { display: none !important; }

    /* Tab bar */
    .tab-bar {
      display: flex;
      flex-shrink: 0;
      border-bottom: 1px solid var(--vscode-panel-border);
      background: var(--vscode-editorGroupHeader-tabsBackground);
    }
    .tab {
      display: flex; align-items: center; gap: 4px;
      padding: 6px 12px; background: transparent;
      border: none; border-bottom: 2px solid transparent;
      color: var(--vscode-tab-inactiveForeground); cursor: pointer;
      font-size: var(--vscode-font-size); font-family: var(--vscode-font-family);
      white-space: nowrap;
    }
    .tab.active {
      color: var(--vscode-tab-activeForeground);
      border-bottom-color: var(--vscode-focusBorder);
      background: var(--vscode-tab-activeBackground);
    }
    .tab:hover:not(.active) { background: var(--vscode-tab-hoverBackground); }

    /* Panels */
    .panel { flex: 1; overflow: hidden; display: flex; flex-direction: column; }
    .view  { display: flex; flex-direction: column; flex: 1; overflow: hidden; }

    /* Search bar (Explorer filter + History search) */
    .search-bar {
      display: flex; align-items: center; gap: 4px;
      padding: 4px 6px; border-bottom: 1px solid var(--vscode-panel-border); flex-shrink: 0;
    }
    .search-icon { color: var(--vscode-descriptionForeground); flex-shrink: 0; font-size: 14px; }
    .search-input {
      flex: 1; background: transparent; border: none; outline: none;
      color: var(--vscode-foreground); font-size: var(--vscode-font-size);
      font-family: var(--vscode-font-family); padding: 0;
    }
    .search-input::placeholder { color: var(--vscode-descriptionForeground); }
    .search-clear {
      background: transparent; border: none; cursor: pointer;
      color: var(--vscode-descriptionForeground); padding: 0;
      display: flex; align-items: center;
    }
    .search-clear:hover { color: var(--vscode-foreground); }

    /* Tree */
    .tree { flex: 1; overflow-y: auto; padding: 2px 0; }
    .tree-row {
      display: flex; align-items: center; height: 22px;
      padding-right: 8px; cursor: default; user-select: none;
    }
    .tree-row:hover { background: var(--vscode-list-hoverBackground); }
    .indent { flex-shrink: 0; display: inline-block; }
    .toggle {
      flex-shrink: 0; width: 16px; height: 16px;
      display: inline-flex; align-items: center; justify-content: center; font-size: 13px;
    }
    @keyframes spin { to { transform: rotate(360deg); } }
    .codicon-modifier-spin { animation: spin 1.2s linear infinite; display: inline-flex; }
    .tree-icon { flex-shrink: 0; margin-right: 4px; width: 16px; display: inline-flex; align-items: center; justify-content: center; }
    .status-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; margin-right: 5px; }
    .status-dot.connected    { background: var(--vscode-testing-iconPassed, #73c991); }
    .status-dot.disconnected { background: var(--vscode-foreground); opacity: 0.3; }
    .status-dot.connecting   { background: var(--vscode-progressBar-background, #0e70c0); }
    .status-dot.error        { background: var(--vscode-errorForeground, #f48771); }
    .tree-label { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .tree-label mark { background: var(--vscode-editor-findMatchHighlightBackground, rgba(234,92,0,0.33)); color: inherit; border-radius: 2px; }
    .tree-badge { flex-shrink: 0; font-size: 10px; background: var(--vscode-badge-background); color: var(--vscode-badge-foreground); border-radius: 10px; padding: 0 5px; margin-left: 4px; min-width: 18px; text-align: center; }
    .tree-actions { display: none; align-items: center; gap: 1px; flex-shrink: 0; margin-left: 4px; }
    .tree-row:hover .tree-actions { display: flex; }
    .col-type { flex-shrink: 0; font-size: 10px; color: var(--vscode-descriptionForeground); margin-left: 4px; font-style: italic; }
    .col-badge { flex-shrink: 0; font-size: 9px; font-weight: 700; border-radius: 2px; padding: 0 3px; margin-left: 3px; line-height: 14px; }
    .col-badge.pk { background: #cca700; color: #000; }
    .col-badge.fk { background: #007acc; color: #fff; }
    .list-footer { padding: 8px 12px; border-top: 1px solid var(--vscode-panel-border); flex-shrink: 0; }

    /* Buttons */
    .btn { display: inline-flex; align-items: center; gap: 6px; padding: 4px 12px; border: none; cursor: pointer; font-size: var(--vscode-font-size); font-family: var(--vscode-font-family); }
    .btn-primary   { background: var(--vscode-button-background); color: var(--vscode-button-foreground); }
    .btn-primary:hover { background: var(--vscode-button-hoverBackground); }
    .btn-secondary { background: var(--vscode-button-secondaryBackground); color: var(--vscode-button-secondaryForeground); }
    .btn-secondary:hover { background: var(--vscode-button-secondaryHoverBackground); }
    .btn:disabled  { opacity: 0.5; cursor: not-allowed; }
    .btn-icon { display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 22px; background: transparent; border: none; cursor: pointer; color: var(--vscode-icon-foreground); border-radius: 2px; }
    .btn-icon:hover { background: var(--vscode-toolbar-hoverBackground); }
    .btn-icon.btn-danger:hover { color: var(--vscode-errorForeground); }

    /* Empty state */
    .empty-state { display: flex; flex-direction: column; align-items: center; justify-content: center; flex: 1; gap: 12px; color: var(--vscode-descriptionForeground); padding: 24px; text-align: center; }

    /* Connection form */
    .form-header { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--vscode-panel-border); flex-shrink: 0; }
    .form-header span { font-weight: 500; }
    .form-body { padding: 12px; overflow-y: auto; flex: 1; }
    .form-group { display: flex; flex-direction: column; gap: 4px; margin-bottom: 12px; }
    .form-row { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
    label { font-size: 11px; color: var(--vscode-descriptionForeground); text-transform: uppercase; letter-spacing: 0.5px; }
    input[type="text"], input[type="number"], input[type="password"] {
      padding: 4px 8px; background: var(--vscode-input-background); color: var(--vscode-input-foreground);
      border: 1px solid var(--vscode-input-border, transparent); outline: none;
      font-size: var(--vscode-font-size); font-family: var(--vscode-font-family); width: 100%;
    }
    input:focus { border-color: var(--vscode-focusBorder); }
    .checkbox-row { display: flex; align-items: center; gap: 8px; }
    .checkbox-row label { text-transform: none; letter-spacing: normal; font-size: var(--vscode-font-size); color: var(--vscode-foreground); cursor: pointer; }
    .form-actions { display: flex; align-items: center; gap: 8px; padding-top: 12px; border-top: 1px solid var(--vscode-panel-border); margin-top: 4px; flex-wrap: wrap; }
    .test-status { font-size: 11px; flex: 1; }
    .test-status.success { color: var(--vscode-testing-iconPassed, #73c991); }
    .test-status.error   { color: var(--vscode-errorForeground); }
    .test-status.testing { color: var(--vscode-descriptionForeground); }

    /* ── Query panel (#15, #16, #17) ─────────────────────────────── */
    .query-tabs-bar {
      display: flex; align-items: stretch; flex-shrink: 0;
      border-bottom: 1px solid var(--vscode-panel-border);
      background: var(--vscode-editorGroupHeader-tabsBackground);
      overflow-x: auto;
    }
    .query-tab-list { display: flex; flex: 1; overflow-x: auto; }
    .query-tab {
      display: flex; align-items: center; gap: 4px;
      padding: 5px 10px; border-right: 1px solid var(--vscode-panel-border);
      cursor: pointer; white-space: nowrap; min-width: 80px; max-width: 140px;
      font-size: var(--vscode-font-size);
    }
    .query-tab:hover { background: var(--vscode-tab-hoverBackground); }
    .query-tab.active {
      background: var(--vscode-tab-activeBackground);
      color: var(--vscode-tab-activeForeground);
      border-bottom: 2px solid var(--vscode-focusBorder);
    }
    .query-tab-name { flex: 1; overflow: hidden; text-overflow: ellipsis; }
    .query-tab-close { width: 14px; height: 14px; font-size: 11px; flex-shrink: 0; padding: 0; background: transparent; border: none; cursor: pointer; color: var(--vscode-icon-foreground); border-radius: 2px; display: inline-flex; align-items: center; justify-content: center; }
    .query-tab-close:hover { background: var(--vscode-toolbar-hoverBackground); }
    .btn-new-tab { flex-shrink: 0; align-self: center; margin: 0 4px; }

    .query-toolbar {
      display: flex; align-items: center; gap: 6px;
      padding: 4px 8px; border-bottom: 1px solid var(--vscode-panel-border); flex-shrink: 0;
    }
    .conn-select {
      flex: 1; background: var(--vscode-dropdown-background); color: var(--vscode-dropdown-foreground);
      border: 1px solid var(--vscode-dropdown-border, transparent); padding: 2px 4px;
      font-size: var(--vscode-font-size); font-family: var(--vscode-font-family); outline: none;
      max-width: 180px;
    }

    #monaco-container { flex: 1; min-height: 120px; overflow: hidden; }

    .query-status {
      padding: 2px 8px; font-size: 11px; flex-shrink: 0;
      color: var(--vscode-descriptionForeground);
      background: var(--vscode-editorGroupHeader-tabsBackground);
      border-top: 1px solid var(--vscode-panel-border);
      white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    }
    .query-status.error { color: var(--vscode-errorForeground); }

    .results-section {
      flex-shrink: 0; max-height: 220px; overflow: auto;
      border-top: 1px solid var(--vscode-panel-border);
    }
    .result-grid { width: max-content; min-width: 100%; border-collapse: collapse; }
    .result-grid th {
      position: sticky; top: 0; z-index: 1;
      background: var(--vscode-editorGroupHeader-tabsBackground);
      padding: 3px 8px; text-align: left; font-weight: 600;
      border-bottom: 2px solid var(--vscode-panel-border); white-space: nowrap; font-size: 11px;
    }
    .result-grid td {
      padding: 2px 8px; border-bottom: 1px solid rgba(128,128,128,0.1);
      white-space: nowrap; font-size: 11px; max-width: 300px; overflow: hidden; text-overflow: ellipsis;
    }
    .result-grid tr:hover { background: var(--vscode-list-hoverBackground); }
    .result-null { color: var(--vscode-descriptionForeground); font-style: italic; }

    /* ── EXPLAIN plan tree (#29) ─────────────────────────────────── */
    .plan-wrap { padding: 8px; overflow: auto; font-size: 11px; }
    .plan-summary { margin-bottom: 8px; color: var(--vscode-descriptionForeground); font-size: 11px; }
    .plan-node { padding: 1px 0 1px 12px; border-left: 1px solid rgba(128,128,128,0.2); margin: 1px 0; }
    .plan-header { display: flex; align-items: baseline; flex-wrap: wrap; gap: 6px; padding: 3px 4px; border-radius: 2px; cursor: pointer; }
    .plan-header:hover { background: var(--vscode-list-hoverBackground); }
    .plan-type { font-weight: 600; }
    .plan-rel  { color: var(--vscode-textLink-foreground); }
    .plan-cost { color: var(--vscode-descriptionForeground); font-size: 10px; }
    .plan-time { font-size: 10px; color: var(--vscode-descriptionForeground); }
    .plan-rows { font-size: 10px; color: var(--vscode-descriptionForeground); }
    .plan-warn .plan-type { color: var(--vscode-editorWarning-foreground, #cca700); }
    .plan-danger .plan-type { color: var(--vscode-errorForeground, #f48771); }
    .plan-children { padding-left: 0; }
    .plan-raw { font-family: var(--vscode-editor-font-family,monospace); font-size: 11px; white-space: pre-wrap; padding: 8px; overflow: auto; }
    .result-toolbar { display: flex; align-items: center; gap: 4px; padding: 3px 6px; border-bottom: 1px solid var(--vscode-panel-border); background: var(--vscode-editorGroupHeader-tabsBackground); flex-shrink: 0; }
    .result-toolbar-spacer { flex: 1; }

    /* ── History panel (#18) ─────────────────────────────────────── */
    .history-list { flex: 1; overflow-y: auto; }
    .history-item { padding: 6px 12px; border-bottom: 1px solid rgba(128,128,128,0.08); cursor: pointer; }
    .history-item:hover { background: var(--vscode-list-hoverBackground); }
    .history-sql { font-family: var(--vscode-editor-font-family, monospace); font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-bottom: 3px; }
    .history-meta { display: flex; gap: 8px; font-size: 10px; color: var(--vscode-descriptionForeground); }

    /* Export bar (#23) */
    .export-bar { display: flex; align-items: center; gap: 4px; padding: 3px 6px; border-bottom: 1px solid var(--vscode-panel-border); background: var(--vscode-editorGroupHeader-tabsBackground); flex-shrink: 0; }
    .btn-xs { padding: 1px 7px; font-size: 11px; }

    /* ── draco panel (#24–#28) ─────────────────────────────────── */
    .draco-scroll { flex: 1; overflow-y: auto; }
    .draco-section { border-bottom: 1px solid var(--vscode-panel-border); }
    .draco-section-header {
      display: flex; align-items: center; gap: 6px; padding: 6px 10px;
      font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.5px;
      color: var(--vscode-descriptionForeground); cursor: pointer; user-select: none;
      background: var(--vscode-sideBarSectionHeader-background);
    }
    .draco-section-header:hover { background: var(--vscode-list-hoverBackground); }
    .draco-section-body { padding: 6px 10px; }
    .draco-info-row { display: flex; align-items: baseline; gap: 6px; padding: 2px 0; font-size: 12px; }
    .draco-info-label { color: var(--vscode-descriptionForeground); font-size: 11px; flex-shrink: 0; }
    .draco-actions-row { display: flex; gap: 6px; padding: 6px 0 2px; }
    .model-row { display: flex; align-items: center; gap: 5px; padding: 3px 0; font-size: 12px; }
    .model-name { font-weight: 500; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .model-map  { color: var(--vscode-descriptionForeground); font-size: 11px; }
    .model-status-ok   { color: var(--vscode-testing-iconPassed,#73c991); font-size: 11px; }
    .model-status-miss { color: var(--vscode-errorForeground,#f48771); font-size: 11px; }
    .model-status-unk  { color: var(--vscode-descriptionForeground); font-size: 11px; }
    .migration-row { display: flex; align-items: center; gap: 8px; padding: 4px 0; font-size: 11px; border-bottom: 1px solid rgba(128,128,128,0.07); }
    .migration-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--vscode-editor-font-family,monospace); font-size: 11px; }
    .migration-date { flex-shrink: 0; color: var(--vscode-descriptionForeground); }
    .mig-ok   { color: var(--vscode-testing-iconPassed,#73c991); flex-shrink: 0; }
    .mig-fail { color: var(--vscode-errorForeground,#f48771); flex-shrink: 0; }
    .mig-rb   { color: var(--vscode-editorWarning-foreground,#cca700); flex-shrink: 0; }
    .draco-log { flex: 1; overflow-y: auto; font-family: var(--vscode-editor-font-family,monospace); font-size: 11px; padding: 8px 10px; white-space: pre-wrap; word-break: break-all; min-height: 80px; max-height: 200px; background: var(--vscode-terminal-background,var(--vscode-editor-background)); color: var(--vscode-terminal-foreground,var(--vscode-foreground)); }
    .conn-select-sm { background: var(--vscode-dropdown-background); color: var(--vscode-dropdown-foreground); border: 1px solid var(--vscode-dropdown-border,transparent); padding: 2px 4px; font-size: var(--vscode-font-size); font-family: var(--vscode-font-family); outline: none; width: 100%; }
    .no-schema-msg { color: var(--vscode-descriptionForeground); font-style: italic; font-size: 12px; padding: 4px 0; }

    /* Placeholder */
    .placeholder { display: flex; align-items: center; justify-content: center; flex: 1; color: var(--vscode-descriptionForeground); font-style: italic; padding: 24px; }
  </style>
</head>
<body>

  <!-- Tab bar -->
  <div class="tab-bar">
    <button class="tab" data-tab="explorer" title="Explorer"><i class="codicon codicon-database"></i></button>
    <button class="tab" data-tab="query" title="Query"><i class="codicon codicon-file-code"></i></button>
    <button class="tab" data-tab="history" title="History"><i class="codicon codicon-history"></i></button>
    <button class="tab" data-tab="draco" id="tab-btn-draco" title="Parser"><i class="codicon codicon-symbol-class"></i></button>
  </div>

  <!-- ── Explorer panel ───────────────────────────────────────────── -->
  <div class="panel" id="panel-explorer">
    <div class="view" id="view-tree">
      <div class="search-bar">
        <i class="search-icon codicon codicon-search"></i>
        <input class="search-input" id="search-input" type="text" placeholder="Filter tables, views, functions…">
        <button class="search-clear hidden" id="search-clear" title="Clear"><i class="codicon codicon-close"></i></button>
      </div>
      <div class="tree" id="tree"></div>
      <div class="list-footer">
        <button class="btn btn-secondary" id="btn-add-conn"><i class="codicon codicon-add"></i>Add Connection</button>
      </div>
    </div>

    <div class="view hidden" id="view-form">
      <div class="form-header">
        <button class="btn-icon" id="btn-back" title="Back"><i class="codicon codicon-arrow-left"></i></button>
        <span id="form-title">Add Connection</span>
      </div>
      <div class="form-body">
        <form id="conn-form" autocomplete="off">
          <div class="form-group">
            <label for="f-label">Label</label>
            <input type="text" id="f-label" placeholder="My Database" required>
          </div>
          <div class="form-row">
            <div class="form-group">
              <label for="f-host">Host</label>
              <input type="text" id="f-host" placeholder="localhost" required>
            </div>
            <div class="form-group">
              <label for="f-port">Port</label>
              <input type="number" id="f-port" value="5432" min="1" max="65535" required>
            </div>
          </div>
          <div class="form-group">
            <label for="f-database">Database</label>
            <input type="text" id="f-database" placeholder="postgres" required>
          </div>
          <div class="form-row">
            <div class="form-group">
              <label for="f-user">User</label>
              <input type="text" id="f-user" placeholder="postgres" required>
            </div>
            <div class="form-group">
              <label for="f-password">Password</label>
              <input type="password" id="f-password" placeholder="Leave blank to keep">
            </div>
          </div>
          <div class="form-group">
            <div class="checkbox-row">
              <input type="checkbox" id="f-ssl">
              <label for="f-ssl">Enable SSL</label>
            </div>
          </div>
          <div class="form-actions">
            <button type="button" class="btn btn-secondary" id="btn-test"><i class="codicon codicon-plug"></i>Test</button>
            <span class="test-status" id="test-status"></span>
            <button type="submit" class="btn btn-primary"><i class="codicon codicon-save"></i>Save</button>
          </div>
        </form>
      </div>
    </div>
  </div>

  <!-- ── Query panel (#15 #16 #17) ────────────────────────────────── -->
  <div class="panel hidden" id="panel-query">
    <div class="query-tabs-bar">
      <div class="query-tab-list" id="query-tab-list"></div>
      <button class="btn-icon btn-new-tab" id="btn-new-tab" title="New query tab"><i class="codicon codicon-add"></i></button>
    </div>
    <div class="query-toolbar">
      <select class="conn-select" id="conn-select"><option value="">Select connection…</option></select>
      <button class="btn btn-primary" id="btn-run"><i class="codicon codicon-play"></i>Run</button>
      <button class="btn btn-secondary" id="btn-explain"><i class="codicon codicon-graph"></i>Explain</button>
    </div>
    <div id="monaco-container"></div>
    <div class="query-status hidden" id="query-status"></div>
    <div class="results-section hidden" id="results-section"></div>
  </div>

  <!-- ── History panel (#18) ──────────────────────────────────────── -->
  <div class="panel hidden" id="panel-history">
    <div class="search-bar">
      <i class="search-icon codicon codicon-search"></i>
      <input class="search-input" id="history-search" type="text" placeholder="Search history…">
      <button class="search-clear hidden" id="history-clear" title="Clear"><i class="codicon codicon-close"></i></button>
    </div>
    <div class="history-list" id="history-list">
      <div class="empty-state"><span>No history yet.</span></div>
    </div>
  </div>

  <!-- ── draco panel (#24–#28) ──────────────────────────────────── -->
  <div class="panel hidden" id="panel-draco">
    <div class="draco-scroll">

      <!-- Connection selector -->
      <div class="draco-section">
        <div class="draco-section-body" style="padding:6px 10px">
          <select class="conn-select-sm" id="draco-conn-select">
            <option value="">Select connection…</option>
          </select>
        </div>
      </div>

      <!-- Schema file -->
      <div class="draco-section">
        <div class="draco-section-header"><i class="codicon codicon-file-code"></i>Schema File</div>
        <div class="draco-section-body" id="draco-schema-body">
          <div class="no-schema-msg">No schema.draco found in workspace.</div>
        </div>
      </div>

      <!-- Models -->
      <div class="draco-section">
        <div class="draco-section-header"><i class="codicon codicon-symbol-class"></i>Models <span id="draco-models-badge" style="margin-left:4px"></span></div>
        <div class="draco-section-body" id="draco-models-body">
          <div class="no-schema-msg">No schema loaded.</div>
        </div>
      </div>

      <!-- Migrations -->
      <div class="draco-section">
        <div class="draco-section-header"><i class="codicon codicon-history"></i>Migrations</div>
        <div class="draco-section-body" id="draco-migrations-body">
          <div class="no-schema-msg">Select a connection to load migrations.</div>
        </div>
      </div>

      <!-- Log -->
      <div class="draco-section">
        <div class="draco-section-header"><i class="codicon codicon-terminal"></i>Log</div>
        <div class="draco-log" id="draco-log">Ready.</div>
      </div>

    </div>
  </div>

  <script nonce="${nonce}" src="${MONACO_CDN}/loader.js"></script>
  <script nonce="${nonce}">
  (function () {
    var vscode = acquireVsCodeApi();

    // ── Persisted & ephemeral state ──────────────────────────────────
    var persisted = vscode.getState() || { tab: 'explorer', tabs: null, activeTabId: null };

    // Explorer state
    var connections  = [];
    var statuses     = {};
    var schemas      = {};
    var tables       = {};
    var cols         = {};
    var funcs        = {};
    var funcParams   = {};
    var expanded     = {};
    var loadingNodes = {};
    var filter       = '';
    var editingId    = null;

    // Query state (#17)
    var tabs = persisted.tabs && persisted.tabs.length
      ? persisted.tabs
      : [{ id: 'qtab-1', name: 'Query 1', sql: '-- Write your query here\\nSELECT 1;', connId: null, result: null }];
    var activeQTabId = persisted.activeTabId || tabs[0].id;

    // Monaco (#15)
    var monacoReady = false;
    var monacoEditor = null;
    var completionProvider = null;

    // History (#18)
    var historyEntries = [];
    var filteredHistory = [];

    // Export (#23) — last successful result
    var lastResult = null;

    // Settings (#30)
    var settings = { defaultPort: 5432, defaultSsl: false, showRowCount: false };

    // Row count estimates (#30)
    var estimates = {};

    // Explain mode toggle (#29) — 'tree' | 'json'
    var explainViewMode = 'tree';

    // ── Helpers ──────────────────────────────────────────────────────
    function esc(s) {
      return String(s)
        .replace(/&/g, '&amp;')
        .replace(/\x3c/g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
    }

    function hl(text, f) {
      if (!f) return esc(text);
      var idx = text.toLowerCase().indexOf(f);
      if (idx < 0) return esc(text);
      return esc(text.slice(0, idx))
        + '<mark>' + esc(text.slice(idx, idx + f.length)) + '</mark>'
        + esc(text.slice(idx + f.length));
    }

    function btnIcon(action, data, icon, title, danger) {
      var attrs = ' data-action="' + esc(action) + '"';
      for (var k in data) {
        if (Object.prototype.hasOwnProperty.call(data, k)) {
          var attr = k.replace(/([A-Z])/g, function (m) { return '-' + m.toLowerCase(); });
          attrs += ' data-' + attr + '="' + esc(String(data[k])) + '"';
        }
      }
      return '<button class="btn-icon' + (danger ? ' btn-danger' : '') + '"' + attrs + ' title="' + esc(title) + '">'
        + '<i class="codicon ' + icon + '"></i></button>';
    }

    function loadingRow(level) {
      return '<div class="tree-row"><span class="indent" style="width:' + (level * 16) + 'px"></span>'
        + '<i class="codicon codicon-loading codicon-modifier-spin" style="margin-right:6px;font-size:13px;"></i>'
        + '<span class="tree-label" style="color:var(--vscode-descriptionForeground)">Loading…</span></div>';
    }

    function infoRow(level, text) {
      return '<div class="tree-row"><span class="indent" style="width:' + (level * 16) + 'px"></span>'
        + '<span class="toggle"></span>'
        + '<span class="tree-label" style="color:var(--vscode-descriptionForeground);font-style:italic">' + esc(text) + '</span></div>';
    }

    function saveState() {
      persisted.tab = persisted.tab || 'explorer';
      persisted.tabs = tabs.map(function (t) { return { id: t.id, name: t.name, sql: t.sql, connId: t.connId }; });
      persisted.activeTabId = activeQTabId;
      vscode.setState(persisted);
    }

    // ── Tree rendering (Explorer) ─────────────────────────────────────
    function renderTree() {
      var treeEl = document.getElementById('tree');
      if (!treeEl) return;
      if (connections.length === 0) {
        treeEl.innerHTML = '<div class="empty-state"><span>No connections yet.</span>'
          + '<button class="btn btn-primary" id="btn-add-first"><i class="codicon codicon-add"></i>Add Connection</button></div>';
        var b = document.getElementById('btn-add-first');
        if (b) b.addEventListener('click', function () { showForm(null); });
        return;
      }
      var scroll = treeEl.scrollTop;
      var f = filter.trim().toLowerCase();
      var html = '';
      connections.forEach(function (conn) { html += renderConn(conn, f); });
      treeEl.innerHTML = html;
      treeEl.scrollTop = scroll;
    }

    function renderConn(conn, f) {
      var nid = 'c:' + conn.id;
      var status = statuses[conn.id] || 'disconnected';
      var isConn = status === 'connected';
      var isExp  = !!expanded[nid];
      var toggleCls = status === 'connecting' ? 'codicon-loading codicon-modifier-spin'
                    : isConn ? (isExp ? 'codicon-chevron-down' : 'codicon-chevron-right') : '';
      var acts = '';
      if (status === 'disconnected' || status === 'error') acts += btnIcon('connect', { connId: conn.id }, 'codicon-plug', 'Connect');
      else if (status === 'connected') acts += btnIcon('disconnect', { connId: conn.id }, 'codicon-debug-disconnect', 'Disconnect');
      acts += btnIcon('edit-conn', { connId: conn.id }, 'codicon-edit', 'Edit');
      acts += btnIcon('delete-conn', { connId: conn.id }, 'codicon-trash', 'Delete', true);

      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:0px"></span>'
        + '<span class="toggle codicon ' + toggleCls + '"></span>'
        + '<span class="status-dot ' + status + '"></span>'
        + '<i class="tree-icon codicon codicon-server"></i>'
        + '<span class="tree-label">' + hl(conn.label, f) + '</span>'
        + '<div class="tree-actions">' + acts + '</div></div>';

      if (isExp && isConn) {
        var sl = schemas[conn.id] || [];
        if (loadingNodes[nid]) { html += loadingRow(1); }
        else if (sl.length === 0) { html += infoRow(1, 'No schemas'); }
        else { sl.forEach(function (s) { html += renderSchema(conn.id, s.name, f); }); }
      }
      return html;
    }

    function renderSchema(connId, schemaName, f) {
      var nid = 's:' + connId + ':' + schemaName;
      var isExp = !!expanded[nid];
      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:16px"></span>'
        + '<span class="toggle codicon ' + (isExp ? 'codicon-chevron-down' : 'codicon-chevron-right') + '"></span>'
        + '<i class="tree-icon codicon codicon-symbol-namespace"></i>'
        + '<span class="tree-label">' + hl(schemaName, f) + '</span>'
        + '<div class="tree-actions"></div></div>';
      if (isExp) {
        html += renderTablesGroup(connId, schemaName, f);
        html += renderFuncsGroup(connId, schemaName, f);
      }
      return html;
    }

    function renderTablesGroup(connId, schemaName, f) {
      var nid = 'tg:' + connId + ':' + schemaName;
      var isExp = !!expanded[nid];
      var tList = tables[connId + ':' + schemaName];
      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:32px"></span>'
        + '<span class="toggle codicon ' + (isExp ? 'codicon-chevron-down' : 'codicon-chevron-right') + '"></span>'
        + '<i class="tree-icon codicon codicon-list-flat"></i>'
        + '<span class="tree-label">Tables &amp; Views</span>'
        + (tList ? '<span class="tree-badge">' + tList.length + '</span>' : '')
        + '<div class="tree-actions"></div></div>';
      if (isExp) {
        if (loadingNodes[nid]) { html += loadingRow(3); }
        else if (!tList) { html += infoRow(3, 'Loading…'); }
        else {
          var filtered = f ? tList.filter(function (t) { return t.name.toLowerCase().indexOf(f) >= 0; }) : tList;
          if (!filtered.length) { html += infoRow(3, f ? 'No matches' : 'Empty schema'); }
          else { filtered.forEach(function (t) { html += renderTableNode(connId, schemaName, t, f); }); }
        }
      }
      return html;
    }

    function renderFuncsGroup(connId, schemaName, f) {
      var nid = 'fg:' + connId + ':' + schemaName;
      var isExp = !!expanded[nid];
      var fList = funcs[connId + ':' + schemaName];
      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:32px"></span>'
        + '<span class="toggle codicon ' + (isExp ? 'codicon-chevron-down' : 'codicon-chevron-right') + '"></span>'
        + '<i class="tree-icon codicon codicon-symbol-method"></i>'
        + '<span class="tree-label">Functions</span>'
        + (fList ? '<span class="tree-badge">' + fList.length + '</span>' : '')
        + '<div class="tree-actions"></div></div>';
      if (isExp) {
        if (loadingNodes[nid]) { html += loadingRow(3); }
        else if (!fList) { html += infoRow(3, 'Loading…'); }
        else {
          var filtered = f ? fList.filter(function (fn) { return fn.name.toLowerCase().indexOf(f) >= 0; }) : fList;
          if (!filtered.length) { html += infoRow(3, f ? 'No matches' : 'No functions'); }
          else { filtered.forEach(function (fn) { html += renderFuncNode(connId, schemaName, fn, f); }); }
        }
      }
      return html;
    }

    function finddracoModel(tableName) {
      if (!dracoSchema) return null;
      return dracoSchema.models.find(function (m) { return m.tableName === tableName || m.tableName.toLowerCase() === tableName.toLowerCase(); }) || null;
    }

    function renderTableNode(connId, schemaName, t, f) {
      var isView = t.type === 'view';
      var nid = (isView ? 'v:' : 't:') + connId + ':' + schemaName + ':' + t.name;
      var isExp = !!expanded[nid];
      var colList = cols[connId + ':' + schemaName + ':' + t.name];
      var dracoModel = finddracoModel(t.name);
      var acts = btnIcon('preview', { connId: connId, schema: schemaName, table: t.name }, 'codicon-open-preview', 'Preview')
               + btnIcon('open-ddl', { connId: connId, schema: schemaName, table: t.name }, 'codicon-symbol-structure', 'View DDL')
               + btnIcon('copy-name', { name: t.name }, 'codicon-copy', 'Copy name');
      var dracoPin = dracoModel ? '<i class="codicon codicon-symbol-class" title="draco model: ' + esc(dracoModel.name) + '" style="font-size:11px;color:var(--vscode-descriptionForeground);margin-right:2px"></i>' : '';
      var estKey = connId + ':' + schemaName;
      var estVal = settings.showRowCount && estimates[estKey] !== undefined ? estimates[estKey][t.name] : undefined;
      var badgeHtml = estVal !== undefined
        ? '<span class="tree-badge" title="estimated rows">' + (estVal >= 1000 ? (estVal/1000).toFixed(0)+'k' : estVal) + '</span>'
        : (colList ? '<span class="tree-badge">' + colList.length + '</span>' : '');
      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:48px"></span>'
        + '<span class="toggle codicon ' + (isExp ? 'codicon-chevron-down' : 'codicon-chevron-right') + '"></span>'
        + '<i class="tree-icon codicon ' + (isView ? 'codicon-layout' : 'codicon-table') + '"></i>'
        + '<span class="tree-label">' + hl(t.name, f) + '</span>'
        + dracoPin + badgeHtml
        + '<div class="tree-actions">' + acts + '</div></div>';
      if (isExp) {
        if (loadingNodes[nid]) { html += loadingRow(4); }
        else if (!colList) { html += infoRow(4, 'Loading…'); }
        else if (!colList.length) { html += infoRow(4, 'No columns'); }
        else { colList.forEach(function (c) { html += renderColRow(c); }); }
      }
      return html;
    }

    function renderColRow(c) {
      var icon = c.isPrimaryKey ? 'codicon-key' : c.isForeignKey ? 'codicon-link' : 'codicon-symbol-field';
      return '<div class="tree-row"><span class="indent" style="width:64px"></span>'
        + '<span class="toggle" style="width:16px;display:inline-block"></span>'
        + '<i class="tree-icon codicon ' + icon + '"></i>'
        + '<span class="tree-label">' + esc(c.name) + '</span>'
        + (c.isPrimaryKey ? '<span class="col-badge pk">PK</span>' : '')
        + (c.isForeignKey ? '<span class="col-badge fk">FK</span>' : '')
        + '<span class="col-type">' + esc(c.dataType) + '</span></div>';
    }

    function renderFuncNode(connId, schemaName, fn, f) {
      var nid = 'f:' + connId + ':' + schemaName + ':' + fn.specificName;
      var isExp = !!expanded[nid];
      var pList = funcParams[connId + ':' + schemaName + ':' + fn.specificName];
      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:48px"></span>'
        + '<span class="toggle codicon ' + (isExp ? 'codicon-chevron-down' : 'codicon-chevron-right') + '"></span>'
        + '<i class="tree-icon codicon ' + (fn.type === 'FUNCTION' ? 'codicon-symbol-function' : 'codicon-symbol-operator') + '"></i>'
        + '<span class="tree-label">' + hl(fn.name, f) + '</span>'
        + (fn.returnType ? '<span class="col-type">→ ' + esc(fn.returnType) + '</span>' : '')
        + '<div class="tree-actions">' + btnIcon('copy-name', { name: fn.name }, 'codicon-copy', 'Copy name') + '</div></div>';
      if (isExp) {
        if (loadingNodes[nid]) { html += loadingRow(4); }
        else if (!pList) { html += infoRow(4, 'Loading…'); }
        else if (!pList.length) { html += infoRow(4, 'No parameters'); }
        else {
          pList.forEach(function (p) {
            html += '<div class="tree-row"><span class="indent" style="width:64px"></span>'
              + '<span class="toggle" style="width:16px;display:inline-block"></span>'
              + '<i class="tree-icon codicon codicon-symbol-variable"></i>'
              + '<span class="tree-label">' + esc(p.name) + '</span>'
              + '<span class="col-type">' + esc(p.mode + ' ' + p.dataType) + '</span></div>';
          });
        }
      }
      return html;
    }

    // ── Node toggle ──────────────────────────────────────────────────
    function toggleNode(nid) {
      if (!nid) return;
      var ci   = nid.indexOf(':');
      var type = nid.slice(0, ci);
      var rest = nid.slice(ci + 1);
      expanded[nid] = !expanded[nid];

      if (expanded[nid]) {
        if (type === 'c') {
          if (!schemas[rest] && !loadingNodes[nid] && statuses[rest] === 'connected') {
            loadingNodes[nid] = true;
            vscode.postMessage({ command: 'loadSchemas', data: { connId: rest } });
          }
        } else if (type === 'tg' || type === 'fg') {
          var p = rest.indexOf(':'); var cId = rest.slice(0, p); var sc = rest.slice(p + 1);
          var key = cId + ':' + sc;
          if (type === 'tg' && !tables[key] && !loadingNodes[nid]) {
            loadingNodes[nid] = true;
            vscode.postMessage({ command: 'loadTables', data: { connId: cId, schema: sc } });
            if (settings.showRowCount && !estimates[key]) {
              vscode.postMessage({ command: 'loadTableEstimates', data: { connId: cId, schema: sc } });
            }
          } else if (type === 'fg' && !funcs[key] && !loadingNodes[nid]) {
            loadingNodes[nid] = true;
            vscode.postMessage({ command: 'loadFunctions', data: { connId: cId, schema: sc } });
          }
        } else if (type === 't' || type === 'v') {
          var parts = rest.split(':'); var cId = parts[0]; var sc = parts[1]; var tbl = parts[2];
          var key = cId + ':' + sc + ':' + tbl;
          if (!cols[key] && !loadingNodes[nid]) {
            loadingNodes[nid] = true;
            vscode.postMessage({ command: 'loadColumns', data: { connId: cId, schema: sc, table: tbl } });
          }
        } else if (type === 'f') {
          var parts = rest.split(':'); var cId = parts[0]; var sc = parts[1]; var sn = parts[2];
          var key = cId + ':' + sc + ':' + sn;
          if (!funcParams[key] && !loadingNodes[nid]) {
            loadingNodes[nid] = true;
            vscode.postMessage({ command: 'loadFuncParams', data: { connId: cId, schema: sc, specificName: sn } });
          }
        }
      }
      renderTree();
    }

    document.getElementById('tree').addEventListener('click', function (e) {
      var actionEl = e.target.closest('[data-action]');
      if (actionEl) { e.stopPropagation(); handleTreeAction(actionEl); return; }
      var rowEl = e.target.closest('.tree-row[data-node]');
      if (rowEl && rowEl.dataset.node) toggleNode(rowEl.dataset.node);
    });

    function handleTreeAction(el) {
      var d = el.dataset;
      switch (d.action) {
        case 'connect':
          statuses[d.connId] = 'connecting'; renderTree();
          vscode.postMessage({ command: 'connect', data: { connId: d.connId } }); break;
        case 'disconnect':
          vscode.postMessage({ command: 'disconnect', data: { connId: d.connId } }); break;
        case 'edit-conn': {
          var conn = connections.find(function (c) { return c.id === d.connId; });
          if (conn) showForm(conn); break;
        }
        case 'delete-conn':
          vscode.postMessage({ command: 'deleteConnection', data: { id: d.connId } }); break;
        case 'preview':
          vscode.postMessage({ command: 'previewTable', data: { connId: d.connId, schema: d.schema, table: d.table } }); break;
        case 'open-ddl':
          vscode.postMessage({ command: 'openDDL', data: { connId: d.connId, schema: d.schema, table: d.table } }); break;
        case 'copy-name':
          navigator.clipboard.writeText(d.name).catch(function () {}); break;
      }
    }

    // ── Explorer search ──────────────────────────────────────────────
    var searchInput = document.getElementById('search-input');
    var searchClear = document.getElementById('search-clear');
    searchInput.addEventListener('input', function () { filter = searchInput.value; searchClear.classList.toggle('hidden', !filter); renderTree(); });
    searchInput.addEventListener('keydown', function (e) { if (e.key === 'Escape') { searchInput.value = ''; filter = ''; searchClear.classList.add('hidden'); renderTree(); } });
    searchClear.addEventListener('click', function () { searchInput.value = ''; filter = ''; searchClear.classList.add('hidden'); renderTree(); });

    // ── Tab switching ────────────────────────────────────────────────
    function switchMainTab(tab) {
      persisted.tab = tab;
      saveState();
      document.querySelectorAll('.tab').forEach(function (b) { b.classList.toggle('active', b.dataset.tab === tab); });
      document.querySelectorAll('.panel').forEach(function (p) { p.classList.toggle('hidden', p.id !== 'panel-' + tab); });
      if (tab === 'query') { setTimeout(ensureMonacoReady, 30); renderQueryTabs(); renderConnSelect(); }
      if (tab === 'history') { if (!historyEntries.length) vscode.postMessage({ command: 'loadHistory' }); renderHistory(); }
      if (tab === 'draco') { renderdracoConnSelect(); renderdracoSchemaInfo(); renderdracoModels(); }
    }

    document.querySelectorAll('.tab').forEach(function (btn) {
      btn.addEventListener('click', function () { switchMainTab(btn.dataset.tab); });
    });

    // ── Explorer form ────────────────────────────────────────────────
    function showForm(conn) {
      editingId = conn ? conn.id : null;
      document.getElementById('form-title').textContent = conn ? 'Edit Connection' : 'Add Connection';
      document.getElementById('f-label').value    = conn ? conn.label    : '';
      document.getElementById('f-host').value     = conn ? conn.host     : 'localhost';
      document.getElementById('f-port').value     = conn ? String(conn.port) : String(settings.defaultPort);
      document.getElementById('f-database').value = conn ? conn.database : '';
      document.getElementById('f-user').value     = conn ? conn.user     : '';
      document.getElementById('f-password').value = '';
      document.getElementById('f-ssl').checked    = conn ? conn.ssl      : settings.defaultSsl;
      var ts = document.getElementById('test-status'); ts.textContent = ''; ts.className = 'test-status';
      document.getElementById('view-tree').classList.add('hidden');
      document.getElementById('view-form').classList.remove('hidden');
    }
    function showTree() { editingId = null; document.getElementById('view-form').classList.add('hidden'); document.getElementById('view-tree').classList.remove('hidden'); }
    function getFormData() {
      return { conn: { id: editingId, label: document.getElementById('f-label').value.trim(), host: document.getElementById('f-host').value.trim(), port: parseInt(document.getElementById('f-port').value, 10) || 5432, database: document.getElementById('f-database').value.trim(), user: document.getElementById('f-user').value.trim(), ssl: document.getElementById('f-ssl').checked }, password: document.getElementById('f-password').value };
    }
    document.getElementById('btn-add-conn').addEventListener('click', function () { showForm(null); });
    document.getElementById('btn-back').addEventListener('click', showTree);
    document.getElementById('btn-test').addEventListener('click', function () {
      var ts = document.getElementById('test-status'); ts.textContent = 'Testing…'; ts.className = 'test-status testing';
      vscode.postMessage({ command: 'testConnection', data: getFormData() });
    });
    document.getElementById('conn-form').addEventListener('submit', function (e) { e.preventDefault(); vscode.postMessage({ command: 'saveConnection', data: getFormData() }); });

    // ── Query tabs (#17) ─────────────────────────────────────────────
    function activeQTab() { return tabs.find(function (t) { return t.id === activeQTabId; }) || tabs[0]; }

    function renderQueryTabs() {
      var html = '';
      tabs.forEach(function (t) {
        var isAct = t.id === activeQTabId;
        html += '<div class="query-tab ' + (isAct ? 'active' : '') + '" data-qtab="' + esc(t.id) + '">'
          + '<span class="query-tab-name">' + esc(t.name) + '</span>'
          + (tabs.length > 1 ? '<button class="query-tab-close" data-close-tab="' + esc(t.id) + '" title="Close"><i class="codicon codicon-close"></i></button>' : '')
          + '</div>';
      });
      document.getElementById('query-tab-list').innerHTML = html;
    }

    document.getElementById('query-tab-list').addEventListener('click', function (e) {
      var closeBtn = e.target.closest('[data-close-tab]');
      if (closeBtn) { e.stopPropagation(); closeQTab(closeBtn.dataset.closeTab); return; }
      var tabEl = e.target.closest('[data-qtab]');
      if (tabEl) switchQTab(tabEl.dataset.qtab);
    });

    document.getElementById('btn-new-tab').addEventListener('click', function () {
      var n = tabs.length + 1;
      var t = { id: 'qtab-' + Date.now(), name: 'Query ' + n, sql: '', connId: activeQTab().connId, result: null };
      tabs.push(t);
      switchQTab(t.id);
    });

    function switchQTab(id) {
      if (monacoEditor && activeQTabId) activeQTab().sql = monacoEditor.getValue();
      activeQTabId = id;
      var tab = activeQTab();
      if (monacoEditor) monacoEditor.setValue(tab.sql || '');
      saveState();
      renderQueryTabs();
      renderConnSelect();
      renderResults(tab.result);
      var statusEl = document.getElementById('query-status');
      if (tab.result) {
        statusEl.classList.remove('hidden');
        var isErr = !!tab.result.error;
        statusEl.className = 'query-status' + (isErr ? ' error' : '');
        statusEl.textContent = isErr ? (tab.result.error || 'Error') : (tab.result.rowCount + ' rows · ' + tab.result.durationMs + 'ms');
      } else {
        statusEl.classList.add('hidden');
      }
    }

    function closeQTab(id) {
      var idx = tabs.findIndex(function (t) { return t.id === id; });
      if (idx < 0) return;
      tabs.splice(idx, 1);
      if (activeQTabId === id) switchQTab(tabs[Math.max(0, idx - 1)].id);
      else { saveState(); renderQueryTabs(); }
    }

    // ── Connection select ────────────────────────────────────────────
    function renderConnSelect() {
      var sel = document.getElementById('conn-select');
      var tab = activeQTab();
      sel.innerHTML = '<option value="">Select connection…</option>'
        + connections.map(function (c) {
            var s = statuses[c.id] || 'disconnected';
            var dot = s === 'connected' ? ' ●' : ' ○';
            return '<option value="' + esc(c.id) + '" ' + (tab.connId === c.id ? 'selected' : '') + '>'
              + esc(c.label) + dot + '</option>';
          }).join('');
    }

    document.getElementById('conn-select').addEventListener('change', function () {
      var connId = this.value;
      activeQTab().connId = connId;
      saveState();
      if (connId) vscode.postMessage({ command: 'loadCompletions', data: { connId: connId } });
    });

    // ── Monaco (#15) ─────────────────────────────────────────────────
    function ensureMonacoReady() {
      if (monacoReady) { if (monacoEditor) monacoEditor.layout(); return; }
      var MONACO_BASE = '${MONACO_CDN}';
      window.MonacoEnvironment = {
        getWorkerUrl: function () {
          var code = 'self.MonacoEnvironment={baseUrl:"' + MONACO_BASE + '/"};importScripts("' + MONACO_BASE + '/base/worker/workerMain.js");';
          var blob = new Blob([code], { type: 'application/javascript' });
          return URL.createObjectURL(blob);
        }
      };
      require.config({ paths: { vs: MONACO_BASE } });
      require(['vs/editor/editor.main'], function () {
        monacoReady = true;
        var isDark = document.body.classList.contains('vscode-dark') || document.body.classList.contains('vscode-high-contrast');
        var tab = activeQTab();
        monacoEditor = monaco.editor.create(document.getElementById('monaco-container'), {
          value: tab.sql || '',
          language: 'sql',
          theme: isDark ? 'vs-dark' : 'vs',
          minimap: { enabled: false },
          lineNumbers: 'on',
          fontSize: 13,
          wordWrap: 'on',
          automaticLayout: true,
          scrollBeyondLastLine: false,
          tabSize: 2,
          renderLineHighlight: 'all',
        });
        monacoEditor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, runQuery);
        monacoEditor.onDidChangeModelContent(function () { activeQTab().sql = monacoEditor.getValue(); });
        if (tab.connId) vscode.postMessage({ command: 'loadCompletions', data: { connId: tab.connId } });
      });
    }

    // ── Execute query (#16) ──────────────────────────────────────────
    document.getElementById('btn-run').addEventListener('click', runQuery);
    document.getElementById('btn-explain').addEventListener('click', runExplain);

    function runQuery() {
      var tab = activeQTab();
      if (!tab.connId) { setStatus('No connection selected.', true); return; }
      var sql = monacoEditor
        ? (monacoEditor.getModel().getValueInRange(monacoEditor.getSelection()).trim() || monacoEditor.getValue().trim())
        : tab.sql.trim();
      if (!sql) return;
      document.getElementById('btn-run').disabled = true;
      setStatus('Executing…', false);
      vscode.postMessage({ command: 'executeQuery', data: { connId: tab.connId, sql: sql, tabId: tab.id } });
    }

    function runExplain() {
      var tab = activeQTab();
      if (!tab.connId) { setStatus('No connection selected.', true); return; }
      var sql = monacoEditor
        ? (monacoEditor.getModel().getValueInRange(monacoEditor.getSelection()).trim() || monacoEditor.getValue().trim())
        : tab.sql.trim();
      if (!sql) return;
      document.getElementById('btn-explain').disabled = true;
      document.getElementById('btn-run').disabled = true;
      setStatus('Running EXPLAIN…', false);
      vscode.postMessage({ command: 'executeExplain', data: { connId: tab.connId, sql: sql, tabId: tab.id } });
    }

    function setStatus(msg, isErr) {
      var el = document.getElementById('query-status');
      el.classList.remove('hidden');
      el.textContent = msg;
      el.className = 'query-status' + (isErr ? ' error' : '');
    }

    function renderResults(result) {
      var section = document.getElementById('results-section');
      if (!result || result.error || !result.columns || !result.columns.length) { section.classList.add('hidden'); lastResult = null; return; }
      section.classList.remove('hidden');
      lastResult = result;
      var html = '<div class="export-bar">'
        + '<button class="btn btn-secondary btn-xs" id="btn-export-csv"><i class="codicon codicon-export"></i>CSV</button>'
        + '<button class="btn btn-secondary btn-xs" id="btn-export-json"><i class="codicon codicon-json"></i>JSON</button>'
        + '</div>';
      html += '<table class="result-grid"><thead><tr>';
      result.columns.forEach(function (c) { html += '<th>' + esc(c) + '</th>'; });
      html += '</tr></thead><tbody>';
      (result.rows || []).forEach(function (row) {
        html += '<tr>';
        result.columns.forEach(function (c) {
          var v = row[c];
          if (v === null || v === undefined) { html += '<td><span class="result-null">NULL</span></td>'; }
          else if (typeof v === 'object') { html += '<td>' + esc(JSON.stringify(v)) + '</td>'; }
          else { html += '<td>' + esc(String(v)) + '</td>'; }
        });
        html += '</tr>';
      });
      html += '</tbody></table>';
      section.innerHTML = html;
      var btnCsv = document.getElementById('btn-export-csv');
      var btnJson = document.getElementById('btn-export-json');
      if (btnCsv) btnCsv.addEventListener('click', function () { exportResult('csv'); });
      if (btnJson) btnJson.addEventListener('click', function () { exportResult('json'); });
    }

    function exportResult(format) {
      if (!lastResult) return;
      vscode.postMessage({ command: 'exportResult', data: { format: format, columns: lastResult.columns, rows: lastResult.rows } });
    }

    // ── EXPLAIN plan renderer (#29) ──────────────────────────────────
    function renderExplain(plan) {
      var section = document.getElementById('results-section');
      if (!plan) { section.classList.add('hidden'); return; }
      section.classList.remove('hidden');
      lastResult = null;
      var rootPlan = plan[0];
      var execTime = rootPlan['Execution Time'] || 0;
      var planTime = rootPlan['Planning Time'] || 0;

      function renderNode(node, depth) {
        var nodeType = node['Node Type'] || '';
        var totalCost = (node['Total Cost'] || 0).toFixed(2);
        var startCost = (node['Startup Cost'] || 0).toFixed(2);
        var actualTime = node['Actual Total Time'];
        var actualRows = node['Actual Rows'];
        var relation = node['Relation Name'] || node['Index Name'] || '';
        var isSeqScan = nodeType === 'Seq Scan';
        var isDanger = execTime > 0 && actualTime !== undefined && (actualTime / execTime) > 0.5;
        var isWarn = isSeqScan || (execTime > 0 && actualTime !== undefined && (actualTime / execTime) > 0.2);
        var cls = isDanger ? 'plan-danger' : isWarn ? 'plan-warn' : '';
        var indent = depth * 14;
        var html = '<div class="plan-node" style="padding-left:' + indent + 'px">';
        html += '<div class="plan-header ' + cls + '">';
        html += '<span class="plan-type">' + esc(nodeType) + '</span>';
        if (relation) html += ' <span class="plan-rel">' + esc(relation) + '</span>';
        if (node['Alias'] && node['Alias'] !== relation) html += ' <span class="plan-cost">alias: ' + esc(node['Alias']) + '</span>';
        html += '<span class="plan-cost">cost ' + startCost + '..' + totalCost + '</span>';
        if (actualTime !== undefined) html += '<span class="plan-time">⏱ ' + actualTime.toFixed(2) + 'ms</span>';
        if (actualRows !== undefined) html += '<span class="plan-rows">→ ' + actualRows + ' rows</span>';
        if (node['Rows Removed by Filter']) html += '<span class="plan-cost" style="color:var(--vscode-editorWarning-foreground)">filtered ' + node['Rows Removed by Filter'] + '</span>';
        html += '</div>';
        if (node['Plans'] && node['Plans'].length) {
          html += '<div class="plan-children">';
          node['Plans'].forEach(function(child) { html += renderNode(child, depth + 1); });
          html += '</div>';
        }
        html += '</div>';
        return html;
      }

      var treeHtml = '<div class="plan-summary">Planning: <b>' + planTime.toFixed(2) + 'ms</b> &nbsp; Execution: <b>' + execTime.toFixed(2) + 'ms</b></div>'
        + renderNode(rootPlan.Plan, 0);
      var jsonHtml = '<pre class="plan-raw">' + esc(JSON.stringify(plan, null, 2)) + '</pre>';

      section.innerHTML = '<div class="result-toolbar">'
        + '<span style="font-size:11px;font-weight:600;color:var(--vscode-descriptionForeground)">EXPLAIN PLAN</span>'
        + '<span class="result-toolbar-spacer"></span>'
        + '<button class="btn btn-secondary btn-xs" id="btn-plan-tree" style="font-weight:' + (explainViewMode==='tree'?'700':'400') + '">Tree</button>'
        + '<button class="btn btn-secondary btn-xs" id="btn-plan-json" style="font-weight:' + (explainViewMode==='json'?'700':'400') + '">JSON</button>'
        + '</div>'
        + '<div class="plan-wrap" id="plan-tree-view">' + treeHtml + '</div>'
        + '<div id="plan-json-view" style="display:none">' + jsonHtml + '</div>';

      if (explainViewMode === 'json') {
        document.getElementById('plan-tree-view').style.display = 'none';
        document.getElementById('plan-json-view').style.display = '';
      }

      document.getElementById('btn-plan-tree').addEventListener('click', function() {
        explainViewMode = 'tree';
        document.getElementById('plan-tree-view').style.display = '';
        document.getElementById('plan-json-view').style.display = 'none';
        document.getElementById('btn-plan-tree').style.fontWeight = '700';
        document.getElementById('btn-plan-json').style.fontWeight = '400';
      });
      document.getElementById('btn-plan-json').addEventListener('click', function() {
        explainViewMode = 'json';
        document.getElementById('plan-tree-view').style.display = 'none';
        document.getElementById('plan-json-view').style.display = '';
        document.getElementById('btn-plan-tree').style.fontWeight = '400';
        document.getElementById('btn-plan-json').style.fontWeight = '700';
      });
    }

    // ── History (#18) ────────────────────────────────────────────────
    var historySearch = document.getElementById('history-search');
    var historyClear  = document.getElementById('history-clear');
    historySearch.addEventListener('input', function () { historyClear.classList.toggle('hidden', !historySearch.value); renderHistory(); });
    historySearch.addEventListener('keydown', function (e) { if (e.key === 'Escape') { historySearch.value = ''; historyClear.classList.add('hidden'); renderHistory(); } });
    historyClear.addEventListener('click', function () { historySearch.value = ''; historyClear.classList.add('hidden'); renderHistory(); });

    function renderHistory() {
      var list = document.getElementById('history-list');
      var q = historySearch.value.toLowerCase();
      filteredHistory = q ? historyEntries.filter(function (h) { return h.sql.toLowerCase().indexOf(q) >= 0; }) : historyEntries;
      if (!filteredHistory.length) {
        list.innerHTML = '<div class="empty-state"><span>' + (q ? 'No matches' : 'No history yet.') + '</span></div>';
        return;
      }
      list.innerHTML = filteredHistory.map(function (h, i) {
        var preview = h.sql.replace(/\s+/g, ' ').slice(0, 100);
        var time = new Date(h.timestamp).toLocaleString();
        return '<div class="history-item" data-hidx="' + i + '">'
          + '<div class="history-sql">' + esc(preview) + '</div>'
          + '<div class="history-meta"><span>' + esc(h.connLabel) + '</span><span>' + esc(time) + '</span><span>' + h.rowCount + ' rows</span><span>' + h.durationMs + 'ms</span></div>'
          + '</div>';
      }).join('');
    }

    document.getElementById('history-list').addEventListener('click', function (e) {
      var item = e.target.closest('[data-hidx]');
      if (!item) return;
      var entry = filteredHistory[parseInt(item.dataset.hidx, 10)];
      if (!entry) return;
      activeQTab().sql = entry.sql;
      if (entry.connId) activeQTab().connId = entry.connId;
      if (monacoEditor) monacoEditor.setValue(entry.sql);
      saveState();
      switchMainTab('query');
    });

    // ── Autocomplete (#19) ───────────────────────────────────────────
    function registerCompletions(data) {
      if (!monacoReady) return;
      if (completionProvider) completionProvider.dispose();
      completionProvider = monaco.languages.registerCompletionItemProvider('sql', {
        provideCompletionItems: function (model, position) {
          var word = model.getWordUntilPosition(position);
          var range = { startLineNumber: position.lineNumber, endLineNumber: position.lineNumber, startColumn: word.startColumn, endColumn: word.endColumn };
          var suggestions = [];
          (data.tables || []).forEach(function (t) {
            suggestions.push({ label: t.name, kind: monaco.languages.CompletionItemKind.Class, insertText: t.name, range: range, detail: t.schema + ' · ' + (t.type === 'view' ? 'View' : 'Table') });
            suggestions.push({ label: t.schema + '.' + t.name, kind: monaco.languages.CompletionItemKind.Class, insertText: t.schema + '.' + t.name, range: range, detail: t.type === 'view' ? 'View' : 'Table' });
          });
          (data.functions || []).forEach(function (f) {
            suggestions.push({ label: f.name, kind: monaco.languages.CompletionItemKind.Function, insertText: f.name + '()', range: range, detail: f.schema + ' · Function' });
          });
          return { suggestions: suggestions };
        }
      });
    }

    // ── draco state (#24–#28) ───────────────────────────────────────
    var dracoSchema = null;
    var dracoConnId = '';
    var dracoLog    = 'Ready.';
    var migrations   = [];
    var migrationsHasTable = false;

    function renderdracoConnSelect() {
      var sel = document.getElementById('draco-conn-select');
      sel.innerHTML = '<option value="">Select connection…</option>'
        + connections.map(function (c) {
            var s = statuses[c.id] || 'disconnected';
            return '<option value="' + esc(c.id) + '"' + (dracoConnId === c.id ? ' selected' : '') + '>'
              + esc(c.label) + (s === 'connected' ? ' ●' : ' ○') + '</option>';
          }).join('');
    }

    document.getElementById('draco-conn-select').addEventListener('change', function () {
      dracoConnId = this.value;
      renderdracoModels();
      if (dracoConnId) {
        document.getElementById('draco-migrations-body').innerHTML = '<div class="no-schema-msg">Loading…</div>';
        vscode.postMessage({ command: 'loadMigrations', data: { connId: dracoConnId } });
      } else {
        document.getElementById('draco-migrations-body').innerHTML = '<div class="no-schema-msg">Select a connection to load migrations.</div>';
      }
    });

    function renderdracoSchemaInfo() {
      var body = document.getElementById('draco-schema-body');
      if (!dracoSchema) {
        body.innerHTML = '<div class="no-schema-msg">No schema.draco found in workspace.</div>';
        return;
      }
      var ds = dracoSchema.datasource;
      var shortPath = dracoSchema.filePath.replace(/\\\\/g, '/').split('/').slice(-3).join('/');
      var html = '<div class="draco-info-row"><span class="draco-info-label">File:</span><span title="' + esc(dracoSchema.filePath) + '">' + esc(shortPath) + '</span></div>';
      if (ds) {
        html += '<div class="draco-info-row"><span class="draco-info-label">Provider:</span><span>' + esc(ds.provider) + (ds.isPostgres ? ' ✓' : ' ⚠') + '</span></div>';
        html += '<div class="draco-info-row"><span class="draco-info-label">URL:</span><span style="font-size:10px;color:var(--vscode-descriptionForeground)">' + esc(ds.url.length > 40 ? ds.url.slice(0, 40) + '…' : ds.url) + '</span></div>';
      }
      html += '<div class="draco-actions-row">'
        + '<button class="btn btn-secondary btn-xs" id="btn-db-pull"><i class="codicon codicon-cloud-download"></i>db pull</button>'
        + '<button class="btn btn-secondary btn-xs" id="btn-migrate-status"><i class="codicon codicon-checklist"></i>migrate status</button>'
        + '</div>';
      body.innerHTML = html;
      document.getElementById('btn-db-pull').addEventListener('click', function () {
        if (!dracoConnId) { setStatus('Select a connection in the draco tab first.', true); switchMainTab('draco'); return; }
        dracoLog = '';
        document.getElementById('draco-log').textContent = '';
        vscode.postMessage({ command: 'rundracoCommand', data: { command: 'db-pull', connId: dracoConnId } });
      });
      document.getElementById('btn-migrate-status').addEventListener('click', function () {
        if (!dracoConnId) { setStatus('Select a connection in the draco tab first.', true); switchMainTab('draco'); return; }
        dracoLog = '';
        document.getElementById('draco-log').textContent = '';
        vscode.postMessage({ command: 'rundracoCommand', data: { command: 'migrate-status', connId: dracoConnId } });
      });
    }

    function renderdracoModels() {
      var body = document.getElementById('draco-models-body');
      var badge = document.getElementById('draco-models-badge');
      if (!dracoSchema || !dracoSchema.models.length) {
        body.innerHTML = '<div class="no-schema-msg">No models found.</div>';
        if (badge) badge.textContent = '';
        return;
      }
      if (badge) badge.textContent = '(' + dracoSchema.models.length + ')';
      var html = '';
      dracoSchema.models.forEach(function (model) {
        var found = null; var foundSchema = null;
        if (dracoConnId) {
          Object.keys(tables).forEach(function (key) {
            if (!key.startsWith(dracoConnId + ':')) return;
            var sc = key.slice(dracoConnId.length + 1);
            var tList = tables[key];
            if (tList) {
              var match = tList.find(function (t) { return t.name === model.tableName || t.name.toLowerCase() === model.tableName.toLowerCase(); });
              if (match && !found) { found = match; foundSchema = sc; }
            }
          });
        }
        var mapNote = model.tableName !== model.name ? ' <span class="model-map">→ ' + esc(model.tableName) + '</span>' : '';
        var statusHtml = dracoConnId
          ? (found ? '<span class="model-status-ok">✓</span>' : '<span class="model-status-miss">✗</span>')
          : '<span class="model-status-unk">?</span>';
        var driftBtn = (found && foundSchema)
          ? '<button class="btn-icon" data-action="open-drift" data-conn-id="' + esc(dracoConnId) + '" data-schema="' + esc(foundSchema) + '" data-table-name="' + esc(model.tableName) + '" data-model-name="' + esc(model.name) + '" title="View Drift"><i class="codicon codicon-diff"></i></button>'
          : '';
        html += '<div class="model-row">'
          + '<i class="codicon codicon-symbol-class" style="font-size:13px;flex-shrink:0"></i>'
          + '<span class="model-name">' + esc(model.name) + '</span>'
          + mapNote + statusHtml + driftBtn + '</div>';
      });
      body.innerHTML = html;
      body.addEventListener('click', function (e) {
        var btn = e.target.closest('[data-action="open-drift"]');
        if (!btn) return;
        var d = btn.dataset;
        vscode.postMessage({ command: 'openDrift', data: { connId: d.connId, schema: d.schema, tableName: d.tableName, modelName: d.modelName } });
      });
    }

    function renderdracoMigrations() {
      var body = document.getElementById('draco-migrations-body');
      if (!dracoConnId) { body.innerHTML = '<div class="no-schema-msg">Select a connection to load migrations.</div>'; return; }
      if (!migrationsHasTable) { body.innerHTML = '<div class="no-schema-msg">Table _draco_migrations not found in this database.</div>'; return; }
      if (!migrations.length) { body.innerHTML = '<div class="no-schema-msg">No migrations found.</div>'; return; }
      body.innerHTML = migrations.map(function (m) {
        var icon, cls;
        if (m.rolledBackAt) { icon = '↩'; cls = 'mig-rb'; }
        else if (m.logs)     { icon = '✗'; cls = 'mig-fail'; }
        else if (m.finishedAt) { icon = '✓'; cls = 'mig-ok'; }
        else { icon = '⧖'; cls = 'mig-rb'; }
        var date = m.finishedAt ? new Date(m.finishedAt).toLocaleDateString() : (m.logs ? 'FAILED' : 'pending');
        return '<div class="migration-row"><span class="' + cls + '">' + icon + '</span><span class="migration-name" title="' + esc(m.migrationName) + '">' + esc(m.migrationName) + '</span><span class="migration-date">' + esc(date) + '</span></div>';
      }).join('');
    }

    function renderdracoTabBadge() {
      var btn = document.getElementById('tab-btn-draco');
      if (!btn) return;
      var hasSchema = !!dracoSchema;
      var label = hasSchema ? 'draco ●' : 'draco';
      btn.innerHTML = '<i class="codicon codicon-symbol-class"></i>' + label;
    }

    // ── Messages from extension host ─────────────────────────────────
    window.addEventListener('message', function (event) {
      var msg = event.data;
      switch (msg.command) {
        case 'updateConnections':
          connections = msg.data || [];
          renderTree();
          if (!document.getElementById('view-form').classList.contains('hidden')) showTree();
          if (persisted.tab === 'query') { renderConnSelect(); }
          if (persisted.tab === 'draco') { renderdracoConnSelect(); }
          break;
        case 'updateStatuses':
          statuses = msg.data || {}; renderTree(); break;
        case 'connectionStatus': {
          var s = msg.data; statuses[s.id] = s.status; delete loadingNodes['c:' + s.id];
          if (s.status === 'connected') { expanded['c:' + s.id] = true; loadingNodes['c:' + s.id] = true; vscode.postMessage({ command: 'loadSchemas', data: { connId: s.id } }); }
          else if (s.status !== 'connecting') { delete schemas[s.id]; }
          renderTree();
          if (persisted.tab === 'query') renderConnSelect();
          break;
        }
        case 'schemasLoaded': { var d = msg.data; schemas[d.connId] = d.schemas; delete loadingNodes['c:' + d.connId]; renderTree(); break; }
        case 'tablesLoaded':  { var d = msg.data; tables[d.connId + ':' + d.schema] = d.tables; delete loadingNodes['tg:' + d.connId + ':' + d.schema]; renderTree(); break; }
        case 'columnsLoaded': { var d = msg.data; cols[d.connId + ':' + d.schema + ':' + d.table] = d.columns; delete loadingNodes[(d.isView ? 'v:' : 't:') + d.connId + ':' + d.schema + ':' + d.table]; renderTree(); break; }
        case 'functionsLoaded': { var d = msg.data; funcs[d.connId + ':' + d.schema] = d.functions; delete loadingNodes['fg:' + d.connId + ':' + d.schema]; renderTree(); break; }
        case 'funcParamsLoaded': { var d = msg.data; funcParams[d.connId + ':' + d.schema + ':' + d.specificName] = d.params; delete loadingNodes['f:' + d.connId + ':' + d.schema + ':' + d.specificName]; renderTree(); break; }
        case 'testResult': {
          var ts = document.getElementById('test-status');
          if (msg.data.success) { ts.textContent = 'Connected successfully!'; ts.className = 'test-status success'; }
          else { ts.textContent = msg.data.message || 'Connection failed'; ts.className = 'test-status error'; }
          break;
        }
        case 'queryResult': {
          var d = msg.data;
          var tab = tabs.find(function (t) { return t.id === d.tabId; });
          if (tab) { tab.result = d; }
          document.getElementById('btn-run').disabled = false;
          if (!d.tabId || d.tabId === activeQTabId) {
            if (d.error) { setStatus(d.error + (d.errorCode ? ' [' + d.errorCode + ']' : ''), true); renderResults(null); }
            else { setStatus(d.rowCount + ' rows · ' + d.durationMs + 'ms', false); renderResults(d); }
          }
          break;
        }
        case 'historyLoaded': { historyEntries = msg.data || []; renderHistory(); break; }
        case 'completionsLoaded': { registerCompletions(msg.data); break; }
        case 'dracoSchema': {
          dracoSchema = msg.data;
          renderdracoSchemaInfo();
          renderdracoModels();
          renderdracoTabBadge();
          break;
        }
        case 'migrationsLoaded': {
          migrationsHasTable = msg.data.hasTable;
          migrations = msg.data.entries || [];
          renderdracoMigrations();
          break;
        }
        case 'dracoLog': {
          dracoLog += msg.data.text;
          var logEl = document.getElementById('draco-log');
          if (logEl) { logEl.textContent = dracoLog; logEl.scrollTop = logEl.scrollHeight; }
          break;
        }
        case 'settings': {
          settings = Object.assign(settings, msg.data);
          break;
        }
        case 'reconnect': {
          var rData = msg.data;
          statuses[rData.connId] = 'connecting'; renderTree();
          vscode.postMessage({ command: 'connect', data: { connId: rData.connId } });
          break;
        }
        case 'tableEstimatesLoaded': {
          var ed = msg.data;
          if (!estimates[ed.connId + ':' + ed.schema]) estimates[ed.connId + ':' + ed.schema] = {};
          Object.assign(estimates[ed.connId + ':' + ed.schema], ed.estimates);
          renderTree();
          break;
        }
        case 'explainResult': {
          var xd = msg.data;
          document.getElementById('btn-explain').disabled = false;
          document.getElementById('btn-run').disabled = false;
          var xtab = tabs.find(function(t) { return t.id === xd.tabId; });
          if (xtab) xtab.result = xd.error ? { error: xd.error } : { isPlan: true };
          if (!xd.tabId || xd.tabId === activeQTabId) {
            if (xd.error) {
              setStatus('EXPLAIN error: ' + xd.error, true);
              document.getElementById('results-section').classList.add('hidden');
            } else {
              setStatus('Query plan ready', false);
              renderExplain(xd.plan);
            }
          }
          break;
        }
        case 'navigateToTable': {
          var nd = msg.data;
          switchMainTab('explorer');
          expanded['c:' + nd.connId] = true;
          expanded['s:' + nd.connId + ':' + nd.schema] = true;
          expanded['tg:' + nd.connId + ':' + nd.schema] = true;
          var tKey = nd.connId + ':' + nd.schema;
          if (!tables[tKey] && !loadingNodes['tg:' + nd.connId + ':' + nd.schema]) {
            loadingNodes['tg:' + nd.connId + ':' + nd.schema] = true;
            vscode.postMessage({ command: 'loadTables', data: { connId: nd.connId, schema: nd.schema } });
          }
          renderTree();
          break;
        }
      }
    });

    // ── Init ─────────────────────────────────────────────────────────
    switchMainTab(persisted.tab || 'explorer');
    renderTree();
    vscode.postMessage({ command: 'ready' });
  }());
  </script>
</body>
</html>`;
}
