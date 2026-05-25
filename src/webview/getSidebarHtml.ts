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
             style-src ${cspSource} 'unsafe-inline';
             font-src ${cspSource};
             script-src 'nonce-${nonce}';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <link rel="stylesheet" href="${codiconsUri}">
  <title>Prisma4Postgres</title>
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
      display: flex;
      align-items: center;
      gap: 4px;
      padding: 6px 12px;
      background: transparent;
      border: none;
      border-bottom: 2px solid transparent;
      color: var(--vscode-tab-inactiveForeground);
      cursor: pointer;
      font-size: var(--vscode-font-size);
      font-family: var(--vscode-font-family);
      white-space: nowrap;
    }
    .tab.active {
      color: var(--vscode-tab-activeForeground);
      border-bottom-color: var(--vscode-focusBorder);
      background: var(--vscode-tab-activeBackground);
    }
    .tab:hover:not(.active) { background: var(--vscode-tab-hoverBackground); }

    /* Panels */
    .panel {
      flex: 1;
      overflow: hidden;
      display: flex;
      flex-direction: column;
    }

    /* Views inside Explorer */
    .view {
      display: flex;
      flex-direction: column;
      flex: 1;
      overflow: hidden;
    }

    /* Search bar (#14) */
    .search-bar {
      display: flex;
      align-items: center;
      gap: 4px;
      padding: 4px 6px;
      border-bottom: 1px solid var(--vscode-panel-border);
      flex-shrink: 0;
    }
    .search-icon { color: var(--vscode-descriptionForeground); flex-shrink: 0; font-size: 14px; }
    .search-input {
      flex: 1;
      background: transparent;
      border: none;
      outline: none;
      color: var(--vscode-foreground);
      font-size: var(--vscode-font-size);
      font-family: var(--vscode-font-family);
      padding: 0;
    }
    .search-input::placeholder { color: var(--vscode-descriptionForeground); }
    .search-clear {
      background: transparent;
      border: none;
      cursor: pointer;
      color: var(--vscode-descriptionForeground);
      padding: 0;
      display: flex;
      align-items: center;
    }
    .search-clear:hover { color: var(--vscode-foreground); }

    /* Tree (#8–#12) */
    .tree {
      flex: 1;
      overflow-y: auto;
      padding: 2px 0;
    }
    .tree-row {
      display: flex;
      align-items: center;
      height: 22px;
      padding-right: 8px;
      cursor: default;
      user-select: none;
    }
    .tree-row:hover { background: var(--vscode-list-hoverBackground); }
    .indent { flex-shrink: 0; display: inline-block; }
    .toggle {
      flex-shrink: 0;
      width: 16px;
      height: 16px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      font-size: 13px;
    }
    @keyframes spin { to { transform: rotate(360deg); } }
    .codicon-modifier-spin { animation: spin 1.2s linear infinite; display: inline-flex; }
    .tree-icon {
      flex-shrink: 0;
      margin-right: 4px;
      width: 16px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
    }

    /* Status dot (#8) */
    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      flex-shrink: 0;
      margin-right: 5px;
    }
    .status-dot.connected    { background: var(--vscode-testing-iconPassed, #73c991); }
    .status-dot.disconnected { background: var(--vscode-foreground); opacity: 0.3; }
    .status-dot.connecting   { background: var(--vscode-progressBar-background, #0e70c0); }
    .status-dot.error        { background: var(--vscode-errorForeground, #f48771); }

    .tree-label {
      flex: 1;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .tree-label mark {
      background: var(--vscode-editor-findMatchHighlightBackground, rgba(234,92,0,0.33));
      color: inherit;
      border-radius: 2px;
    }
    .tree-badge {
      flex-shrink: 0;
      font-size: 10px;
      background: var(--vscode-badge-background);
      color: var(--vscode-badge-foreground);
      border-radius: 10px;
      padding: 0 5px;
      margin-left: 4px;
      min-width: 18px;
      text-align: center;
    }
    .tree-actions {
      display: none;
      align-items: center;
      gap: 1px;
      flex-shrink: 0;
      margin-left: 4px;
    }
    .tree-row:hover .tree-actions { display: flex; }

    /* Column badges (#11) */
    .col-type {
      flex-shrink: 0;
      font-size: 10px;
      color: var(--vscode-descriptionForeground);
      margin-left: 4px;
      font-style: italic;
    }
    .col-badge {
      flex-shrink: 0;
      font-size: 9px;
      font-weight: 700;
      border-radius: 2px;
      padding: 0 3px;
      margin-left: 3px;
      line-height: 14px;
    }
    .col-badge.pk { background: #cca700; color: #000; }
    .col-badge.fk { background: #007acc; color: #fff; }

    /* List footer */
    .list-footer {
      padding: 8px 12px;
      border-top: 1px solid var(--vscode-panel-border);
      flex-shrink: 0;
    }

    /* Buttons */
    .btn {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 4px 12px;
      border: none;
      cursor: pointer;
      font-size: var(--vscode-font-size);
      font-family: var(--vscode-font-family);
    }
    .btn-primary { background: var(--vscode-button-background); color: var(--vscode-button-foreground); }
    .btn-primary:hover { background: var(--vscode-button-hoverBackground); }
    .btn-secondary { background: var(--vscode-button-secondaryBackground); color: var(--vscode-button-secondaryForeground); }
    .btn-secondary:hover { background: var(--vscode-button-secondaryHoverBackground); }
    .btn-icon {
      display: inline-flex; align-items: center; justify-content: center;
      width: 22px; height: 22px;
      background: transparent; border: none; cursor: pointer;
      color: var(--vscode-icon-foreground); border-radius: 2px;
    }
    .btn-icon:hover { background: var(--vscode-toolbar-hoverBackground); }
    .btn-icon.btn-danger:hover { color: var(--vscode-errorForeground); }

    /* Empty state */
    .empty-state {
      display: flex; flex-direction: column; align-items: center; justify-content: center;
      flex: 1; gap: 12px; color: var(--vscode-descriptionForeground);
      padding: 24px; text-align: center;
    }

    /* Form */
    .form-header {
      display: flex; align-items: center; gap: 8px;
      padding: 8px 12px; border-bottom: 1px solid var(--vscode-panel-border); flex-shrink: 0;
    }
    .form-header span { font-weight: 500; }
    .form-body { padding: 12px; overflow-y: auto; flex: 1; }
    .form-group { display: flex; flex-direction: column; gap: 4px; margin-bottom: 12px; }
    .form-row { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
    label { font-size: 11px; color: var(--vscode-descriptionForeground); text-transform: uppercase; letter-spacing: 0.5px; }
    input[type="text"], input[type="number"], input[type="password"] {
      padding: 4px 8px;
      background: var(--vscode-input-background); color: var(--vscode-input-foreground);
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

    /* Placeholder */
    .placeholder {
      display: flex; align-items: center; justify-content: center; flex: 1;
      color: var(--vscode-descriptionForeground); font-style: italic; padding: 24px;
    }
  </style>
</head>
<body>

  <!-- Tab bar -->
  <div class="tab-bar">
    <button class="tab" data-tab="explorer"><i class="codicon codicon-database"></i>Explorer</button>
    <button class="tab" data-tab="query"><i class="codicon codicon-file-code"></i>Query</button>
    <button class="tab" data-tab="history"><i class="codicon codicon-history"></i>History</button>
  </div>

  <!-- Explorer panel -->
  <div class="panel" id="panel-explorer">

    <!-- Tree view -->
    <div class="view" id="view-tree">
      <div class="search-bar">
        <i class="search-icon codicon codicon-search"></i>
        <input class="search-input" id="search-input" type="text" placeholder="Filter tables, views, functions…">
        <button class="search-clear hidden" id="search-clear" title="Clear filter">
          <i class="codicon codicon-close"></i>
        </button>
      </div>
      <div class="tree" id="tree"></div>
      <div class="list-footer">
        <button class="btn btn-secondary" id="btn-add-conn">
          <i class="codicon codicon-add"></i>Add Connection
        </button>
      </div>
    </div>

    <!-- Form view -->
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
            <button type="button" class="btn btn-secondary" id="btn-test">
              <i class="codicon codicon-plug"></i>Test
            </button>
            <span class="test-status" id="test-status"></span>
            <button type="submit" class="btn btn-primary">
              <i class="codicon codicon-save"></i>Save
            </button>
          </div>
        </form>
      </div>
    </div>
  </div>

  <!-- Query panel -->
  <div class="panel hidden" id="panel-query">
    <div class="placeholder">Query editor — coming in v0.3</div>
  </div>

  <!-- History panel -->
  <div class="panel hidden" id="panel-history">
    <div class="placeholder">Query history — coming in v0.3</div>
  </div>

  <script nonce="${nonce}">
  (function () {
    var vscode = acquireVsCodeApi();

    // ── State ────────────────────────────────────────────────────────
    var persisted    = vscode.getState() || { tab: 'explorer' };
    var connections  = [];
    var statuses     = {};    // { connId: status }
    var schemas      = {};    // { connId: [{name}] }
    var tables       = {};    // { 'connId:schema': [{name,type}] }
    var cols         = {};    // { 'connId:schema:table': [ColumnInfo] }
    var funcs        = {};    // { 'connId:schema': [FunctionInfo] }
    var funcParams   = {};    // { 'connId:schema:specificName': [FunctionParam] }
    var expanded     = {};    // { nodeId: true }
    var loadingNodes = {};    // { nodeId: true }
    var filter       = '';
    var editingId    = null;

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
      return '<button class="btn-icon' + (danger ? ' btn-danger' : '') + '"'
        + attrs + ' title="' + esc(title) + '">'
        + '<i class="codicon ' + icon + '"></i></button>';
    }

    function loadingRow(level) {
      return '<div class="tree-row">'
        + '<span class="indent" style="width:' + (level * 16) + 'px"></span>'
        + '<i class="codicon codicon-loading codicon-modifier-spin" style="margin-right:6px;font-size:13px;"></i>'
        + '<span class="tree-label" style="color:var(--vscode-descriptionForeground)">Loading…</span>'
        + '</div>';
    }

    function infoRow(level, text) {
      return '<div class="tree-row">'
        + '<span class="indent" style="width:' + (level * 16) + 'px"></span>'
        + '<span class="toggle"></span>'
        + '<span class="tree-label" style="color:var(--vscode-descriptionForeground);font-style:italic">' + esc(text) + '</span>'
        + '</div>';
    }

    // ── Tree rendering ───────────────────────────────────────────────
    function renderTree() {
      var treeEl = document.getElementById('tree');
      if (!treeEl) return;
      if (connections.length === 0) {
        treeEl.innerHTML = '<div class="empty-state">'
          + '<span>No connections yet.</span>'
          + '<button class="btn btn-primary" id="btn-add-first"><i class="codicon codicon-add"></i>Add Connection</button>'
          + '</div>';
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
      var isLoad = !!loadingNodes[nid];

      var toggleCls = '';
      if (status === 'connecting') toggleCls = 'codicon-loading codicon-modifier-spin';
      else if (isConn) toggleCls = isExp ? 'codicon-chevron-down' : 'codicon-chevron-right';

      var acts = '';
      if (status === 'disconnected' || status === 'error') {
        acts += btnIcon('connect', { connId: conn.id }, 'codicon-plug', 'Connect');
      } else if (status === 'connected') {
        acts += btnIcon('disconnect', { connId: conn.id }, 'codicon-debug-disconnect', 'Disconnect');
      }
      acts += btnIcon('edit-conn', { connId: conn.id }, 'codicon-edit', 'Edit');
      acts += btnIcon('delete-conn', { connId: conn.id }, 'codicon-trash', 'Delete', true);

      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:0px"></span>'
        + '<span class="toggle codicon ' + toggleCls + '"></span>'
        + '<span class="status-dot ' + status + '"></span>'
        + '<i class="tree-icon codicon codicon-server"></i>'
        + '<span class="tree-label">' + hl(conn.label, f) + '</span>'
        + '<div class="tree-actions">' + acts + '</div>'
        + '</div>';

      if (isExp && isConn) {
        if (isLoad) {
          html += loadingRow(1);
        } else {
          var sl = schemas[conn.id] || [];
          if (sl.length === 0) {
            html += infoRow(1, 'No schemas');
          } else {
            sl.forEach(function (s) { html += renderSchema(conn.id, s.name, f); });
          }
        }
      }
      return html;
    }

    function renderSchema(connId, schemaName, f) {
      var nid = 's:' + connId + ':' + schemaName;
      var isExp = !!expanded[nid];
      var toggleCls = isExp ? 'codicon-chevron-down' : 'codicon-chevron-right';

      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:16px"></span>'
        + '<span class="toggle codicon ' + toggleCls + '"></span>'
        + '<i class="tree-icon codicon codicon-symbol-namespace"></i>'
        + '<span class="tree-label">' + hl(schemaName, f) + '</span>'
        + '<div class="tree-actions"></div>'
        + '</div>';

      if (isExp) {
        html += renderTablesGroup(connId, schemaName, f);
        html += renderFuncsGroup(connId, schemaName, f);
      }
      return html;
    }

    function renderTablesGroup(connId, schemaName, f) {
      var nid = 'tg:' + connId + ':' + schemaName;
      var isExp  = !!expanded[nid];
      var isLoad = !!loadingNodes[nid];
      var toggleCls = isExp ? 'codicon-chevron-down' : 'codicon-chevron-right';
      var tList = tables[connId + ':' + schemaName];
      var badge = tList ? String(tList.length) : '';

      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:32px"></span>'
        + '<span class="toggle codicon ' + toggleCls + '"></span>'
        + '<i class="tree-icon codicon codicon-list-flat"></i>'
        + '<span class="tree-label">Tables &amp; Views</span>'
        + (badge ? '<span class="tree-badge">' + esc(badge) + '</span>' : '')
        + '<div class="tree-actions"></div>'
        + '</div>';

      if (isExp) {
        if (isLoad) {
          html += loadingRow(3);
        } else if (!tList) {
          html += infoRow(3, 'Loading…');
        } else {
          var filtered = f ? tList.filter(function (t) { return t.name.toLowerCase().indexOf(f) >= 0; }) : tList;
          if (filtered.length === 0) {
            html += infoRow(3, f ? 'No matches' : 'Empty schema');
          } else {
            filtered.forEach(function (t) { html += renderTableNode(connId, schemaName, t, f); });
          }
        }
      }
      return html;
    }

    function renderFuncsGroup(connId, schemaName, f) {
      var nid = 'fg:' + connId + ':' + schemaName;
      var isExp  = !!expanded[nid];
      var isLoad = !!loadingNodes[nid];
      var toggleCls = isExp ? 'codicon-chevron-down' : 'codicon-chevron-right';
      var fList = funcs[connId + ':' + schemaName];
      var badge = fList ? String(fList.length) : '';

      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:32px"></span>'
        + '<span class="toggle codicon ' + toggleCls + '"></span>'
        + '<i class="tree-icon codicon codicon-symbol-method"></i>'
        + '<span class="tree-label">Functions</span>'
        + (badge ? '<span class="tree-badge">' + esc(badge) + '</span>' : '')
        + '<div class="tree-actions"></div>'
        + '</div>';

      if (isExp) {
        if (isLoad) {
          html += loadingRow(3);
        } else if (!fList) {
          html += infoRow(3, 'Loading…');
        } else {
          var filtered = f ? fList.filter(function (fn) { return fn.name.toLowerCase().indexOf(f) >= 0; }) : fList;
          if (filtered.length === 0) {
            html += infoRow(3, f ? 'No matches' : 'No functions');
          } else {
            filtered.forEach(function (fn) { html += renderFuncNode(connId, schemaName, fn, f); });
          }
        }
      }
      return html;
    }

    function renderTableNode(connId, schemaName, t, f) {
      var isView = t.type === 'view';
      var nid    = (isView ? 'v:' : 't:') + connId + ':' + schemaName + ':' + t.name;
      var isExp  = !!expanded[nid];
      var isLoad = !!loadingNodes[nid];
      var icon   = isView ? 'codicon-layout' : 'codicon-table';
      var toggleCls = isExp ? 'codicon-chevron-down' : 'codicon-chevron-right';
      var colList = cols[connId + ':' + schemaName + ':' + t.name];
      var badge = colList ? String(colList.length) : '';

      var acts = btnIcon('preview', { connId: connId, schema: schemaName, table: t.name }, 'codicon-open-preview', 'Preview');
      acts += btnIcon('copy-name', { name: t.name }, 'codicon-copy', 'Copy name');

      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:48px"></span>'
        + '<span class="toggle codicon ' + toggleCls + '"></span>'
        + '<i class="tree-icon codicon ' + icon + '"></i>'
        + '<span class="tree-label">' + hl(t.name, f) + '</span>'
        + (badge ? '<span class="tree-badge">' + esc(badge) + '</span>' : '')
        + '<div class="tree-actions">' + acts + '</div>'
        + '</div>';

      if (isExp) {
        if (isLoad) {
          html += loadingRow(4);
        } else if (!colList) {
          html += infoRow(4, 'Loading…');
        } else if (colList.length === 0) {
          html += infoRow(4, 'No columns');
        } else {
          colList.forEach(function (c) { html += renderColRow(c); });
        }
      }
      return html;
    }

    function renderColRow(c) {
      var icon = c.isPrimaryKey ? 'codicon-key' : c.isForeignKey ? 'codicon-link' : 'codicon-symbol-field';
      var badges = '';
      if (c.isPrimaryKey) badges += '<span class="col-badge pk">PK</span>';
      if (c.isForeignKey) badges += '<span class="col-badge fk">FK</span>';
      return '<div class="tree-row">'
        + '<span class="indent" style="width:64px"></span>'
        + '<span class="toggle" style="width:16px;display:inline-block"></span>'
        + '<i class="tree-icon codicon ' + icon + '"></i>'
        + '<span class="tree-label">' + esc(c.name) + '</span>'
        + badges
        + '<span class="col-type">' + esc(c.dataType) + '</span>'
        + '</div>';
    }

    function renderFuncNode(connId, schemaName, fn, f) {
      var nid    = 'f:' + connId + ':' + schemaName + ':' + fn.specificName;
      var isExp  = !!expanded[nid];
      var isLoad = !!loadingNodes[nid];
      var icon   = fn.type === 'FUNCTION' ? 'codicon-symbol-function' : 'codicon-symbol-operator';
      var toggleCls = isExp ? 'codicon-chevron-down' : 'codicon-chevron-right';

      var html = '<div class="tree-row" data-node="' + esc(nid) + '">'
        + '<span class="indent" style="width:48px"></span>'
        + '<span class="toggle codicon ' + toggleCls + '"></span>'
        + '<i class="tree-icon codicon ' + icon + '"></i>'
        + '<span class="tree-label">' + hl(fn.name, f) + '</span>'
        + (fn.returnType ? '<span class="col-type">→ ' + esc(fn.returnType) + '</span>' : '')
        + '<div class="tree-actions">' + btnIcon('copy-name', { name: fn.name }, 'codicon-copy', 'Copy name') + '</div>'
        + '</div>';

      if (isExp) {
        var pKey = connId + ':' + schemaName + ':' + fn.specificName;
        var pList = funcParams[pKey];
        if (isLoad) {
          html += loadingRow(4);
        } else if (!pList) {
          html += infoRow(4, 'Loading…');
        } else if (pList.length === 0) {
          html += infoRow(4, 'No parameters');
        } else {
          pList.forEach(function (p) {
            html += '<div class="tree-row">'
              + '<span class="indent" style="width:64px"></span>'
              + '<span class="toggle" style="width:16px;display:inline-block"></span>'
              + '<i class="tree-icon codicon codicon-symbol-variable"></i>'
              + '<span class="tree-label">' + esc(p.name) + '</span>'
              + '<span class="col-type">' + esc(p.mode + ' ' + p.dataType) + '</span>'
              + '</div>';
          });
        }
      }
      return html;
    }

    // ── Node toggle ──────────────────────────────────────────────────
    function toggleNode(nid) {
      if (!nid) return;
      var ci = nid.indexOf(':');
      var type = nid.slice(0, ci);
      var rest = nid.slice(ci + 1);

      if (expanded[nid]) {
        expanded[nid] = false;
      } else {
        expanded[nid] = true;

        if (type === 'c') {
          var connId = rest;
          if (!schemas[connId] && !loadingNodes[nid] && (statuses[connId] === 'connected')) {
            loadingNodes[nid] = true;
            vscode.postMessage({ command: 'loadSchemas', data: { connId: connId } });
          }
        } else if (type === 'tg') {
          var p = rest.indexOf(':');
          var cId = rest.slice(0, p);
          var sc  = rest.slice(p + 1);
          var key = cId + ':' + sc;
          if (!tables[key] && !loadingNodes[nid]) {
            loadingNodes[nid] = true;
            vscode.postMessage({ command: 'loadTables', data: { connId: cId, schema: sc } });
          }
        } else if (type === 'fg') {
          var p = rest.indexOf(':');
          var cId = rest.slice(0, p);
          var sc  = rest.slice(p + 1);
          var key = cId + ':' + sc;
          if (!funcs[key] && !loadingNodes[nid]) {
            loadingNodes[nid] = true;
            vscode.postMessage({ command: 'loadFunctions', data: { connId: cId, schema: sc } });
          }
        } else if (type === 't' || type === 'v') {
          var parts = rest.split(':');
          var cId = parts[0]; var sc = parts[1]; var tbl = parts[2];
          var key = cId + ':' + sc + ':' + tbl;
          if (!cols[key] && !loadingNodes[nid]) {
            loadingNodes[nid] = true;
            vscode.postMessage({ command: 'loadColumns', data: { connId: cId, schema: sc, table: tbl } });
          }
        } else if (type === 'f') {
          var parts = rest.split(':');
          var cId = parts[0]; var sc = parts[1]; var sn = parts[2];
          var key = cId + ':' + sc + ':' + sn;
          if (!funcParams[key] && !loadingNodes[nid]) {
            loadingNodes[nid] = true;
            vscode.postMessage({ command: 'loadFuncParams', data: { connId: cId, schema: sc, specificName: sn } });
          }
        }
      }
      renderTree();
    }

    // ── Event delegation ─────────────────────────────────────────────
    document.getElementById('tree').addEventListener('click', function (e) {
      var actionEl = e.target.closest('[data-action]');
      if (actionEl) { e.stopPropagation(); handleAction(actionEl); return; }
      var rowEl = e.target.closest('.tree-row[data-node]');
      if (rowEl && rowEl.dataset.node) toggleNode(rowEl.dataset.node);
    });

    function handleAction(el) {
      var action = el.dataset.action;
      var d = el.dataset;
      switch (action) {
        case 'connect':
          statuses[d.connId] = 'connecting';
          renderTree();
          vscode.postMessage({ command: 'connect', data: { connId: d.connId } });
          break;
        case 'disconnect':
          vscode.postMessage({ command: 'disconnect', data: { connId: d.connId } });
          break;
        case 'edit-conn': {
          var conn = connections.find(function (c) { return c.id === d.connId; });
          if (conn) showForm(conn);
          break;
        }
        case 'delete-conn':
          vscode.postMessage({ command: 'deleteConnection', data: { id: d.connId } });
          break;
        case 'preview':
          vscode.postMessage({ command: 'previewTable', data: { connId: d.connId, schema: d.schema, table: d.table } });
          break;
        case 'copy-name':
          navigator.clipboard.writeText(d.name).catch(function () {});
          break;
      }
    }

    // ── Search / filter (#14) ────────────────────────────────────────
    var searchInput = document.getElementById('search-input');
    var searchClear = document.getElementById('search-clear');

    searchInput.addEventListener('input', function () {
      filter = searchInput.value;
      searchClear.classList.toggle('hidden', !filter);
      renderTree();
    });

    searchInput.addEventListener('keydown', function (e) {
      if (e.key === 'Escape') { searchInput.value = ''; filter = ''; searchClear.classList.add('hidden'); renderTree(); }
    });

    searchClear.addEventListener('click', function () {
      searchInput.value = ''; filter = ''; searchClear.classList.add('hidden'); renderTree();
    });

    // ── Tab switching ────────────────────────────────────────────────
    function switchTab(tab) {
      persisted.tab = tab;
      vscode.setState(persisted);
      document.querySelectorAll('.tab').forEach(function (b) { b.classList.toggle('active', b.dataset.tab === tab); });
      document.querySelectorAll('.panel').forEach(function (p) { p.classList.toggle('hidden', p.id !== 'panel-' + tab); });
    }

    document.querySelectorAll('.tab').forEach(function (btn) {
      btn.addEventListener('click', function () { switchTab(btn.dataset.tab); });
    });

    // ── Form helpers ─────────────────────────────────────────────────
    function showForm(conn) {
      editingId = conn ? conn.id : null;
      document.getElementById('form-title').textContent = conn ? 'Edit Connection' : 'Add Connection';
      document.getElementById('f-label').value    = conn ? conn.label    : '';
      document.getElementById('f-host').value     = conn ? conn.host     : 'localhost';
      document.getElementById('f-port').value     = conn ? String(conn.port) : '5432';
      document.getElementById('f-database').value = conn ? conn.database : '';
      document.getElementById('f-user').value     = conn ? conn.user     : '';
      document.getElementById('f-password').value = '';
      document.getElementById('f-ssl').checked    = conn ? conn.ssl      : false;
      var ts = document.getElementById('test-status');
      ts.textContent = ''; ts.className = 'test-status';
      document.getElementById('view-tree').classList.add('hidden');
      document.getElementById('view-form').classList.remove('hidden');
    }

    function showTree() {
      editingId = null;
      document.getElementById('view-form').classList.add('hidden');
      document.getElementById('view-tree').classList.remove('hidden');
    }

    function getFormData() {
      return {
        conn: {
          id:       editingId,
          label:    document.getElementById('f-label').value.trim(),
          host:     document.getElementById('f-host').value.trim(),
          port:     parseInt(document.getElementById('f-port').value, 10) || 5432,
          database: document.getElementById('f-database').value.trim(),
          user:     document.getElementById('f-user').value.trim(),
          ssl:      document.getElementById('f-ssl').checked,
        },
        password: document.getElementById('f-password').value,
      };
    }

    document.getElementById('btn-add-conn').addEventListener('click', function () { showForm(null); });
    document.getElementById('btn-back').addEventListener('click', showTree);

    document.getElementById('btn-test').addEventListener('click', function () {
      var ts = document.getElementById('test-status');
      ts.textContent = 'Testing…'; ts.className = 'test-status testing';
      vscode.postMessage({ command: 'testConnection', data: getFormData() });
    });

    document.getElementById('conn-form').addEventListener('submit', function (e) {
      e.preventDefault();
      vscode.postMessage({ command: 'saveConnection', data: getFormData() });
    });

    // ── Messages from extension host ─────────────────────────────────
    window.addEventListener('message', function (event) {
      var msg = event.data;
      switch (msg.command) {

        case 'updateConnections':
          connections = msg.data || [];
          renderTree();
          if (!document.getElementById('view-form').classList.contains('hidden')) showTree();
          break;

        case 'updateStatuses':
          statuses = msg.data || {};
          renderTree();
          break;

        case 'connectionStatus': {
          var s = msg.data;
          statuses[s.id] = s.status;
          delete loadingNodes['c:' + s.id];
          if (s.status === 'connected') {
            expanded['c:' + s.id] = true;
            loadingNodes['c:' + s.id] = true;
            vscode.postMessage({ command: 'loadSchemas', data: { connId: s.id } });
          } else if (s.status !== 'connecting') {
            delete schemas[s.id];
          }
          renderTree();
          break;
        }

        case 'schemasLoaded': {
          var d = msg.data;
          schemas[d.connId] = d.schemas;
          delete loadingNodes['c:' + d.connId];
          renderTree();
          break;
        }

        case 'tablesLoaded': {
          var d = msg.data;
          var key = d.connId + ':' + d.schema;
          tables[key] = d.tables;
          delete loadingNodes['tg:' + d.connId + ':' + d.schema];
          renderTree();
          break;
        }

        case 'columnsLoaded': {
          var d = msg.data;
          var key = d.connId + ':' + d.schema + ':' + d.table;
          cols[key] = d.columns;
          var nid = (d.isView ? 'v:' : 't:') + d.connId + ':' + d.schema + ':' + d.table;
          delete loadingNodes[nid];
          renderTree();
          break;
        }

        case 'functionsLoaded': {
          var d = msg.data;
          var key = d.connId + ':' + d.schema;
          funcs[key] = d.functions;
          delete loadingNodes['fg:' + d.connId + ':' + d.schema];
          renderTree();
          break;
        }

        case 'funcParamsLoaded': {
          var d = msg.data;
          var key = d.connId + ':' + d.schema + ':' + d.specificName;
          funcParams[key] = d.params;
          delete loadingNodes['f:' + d.connId + ':' + d.schema + ':' + d.specificName];
          renderTree();
          break;
        }

        case 'testResult':
          var ts = document.getElementById('test-status');
          if (msg.data.success) {
            ts.textContent = 'Connected successfully!'; ts.className = 'test-status success';
          } else {
            ts.textContent = msg.data.message || 'Connection failed'; ts.className = 'test-status error';
          }
          break;
      }
    });

    // ── Init ─────────────────────────────────────────────────────────
    switchTab(persisted.tab || 'explorer');
    renderTree();
    vscode.postMessage({ command: 'ready' });
  }());
  </script>
</body>
</html>`;
}
