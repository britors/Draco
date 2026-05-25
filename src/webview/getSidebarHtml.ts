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
    .tab:hover:not(.active) {
      background: var(--vscode-tab-hoverBackground);
    }

    /* Panels */
    .panel {
      flex: 1;
      overflow: hidden;
      display: flex;
      flex-direction: column;
    }

    /* Views inside Explorer panel */
    .view {
      display: flex;
      flex-direction: column;
      flex: 1;
      overflow: hidden;
    }

    /* Empty state */
    .empty-state {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      flex: 1;
      gap: 12px;
      color: var(--vscode-descriptionForeground);
      padding: 24px;
      text-align: center;
    }

    /* Connection list */
    .conn-list {
      flex: 1;
      overflow-y: auto;
    }
    .conn-item {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 6px 12px;
      cursor: default;
    }
    .conn-item:hover {
      background: var(--vscode-list-hoverBackground);
    }
    .conn-item > .codicon {
      color: var(--vscode-descriptionForeground);
      flex-shrink: 0;
    }
    .conn-info {
      flex: 1;
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }
    .conn-label {
      font-weight: 500;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .conn-meta {
      font-size: 11px;
      color: var(--vscode-descriptionForeground);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .conn-actions {
      display: none;
      align-items: center;
      gap: 2px;
      flex-shrink: 0;
    }
    .conn-item:hover .conn-actions {
      display: flex;
    }

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
    .btn-primary {
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
    }
    .btn-primary:hover {
      background: var(--vscode-button-hoverBackground);
    }
    .btn-secondary {
      background: var(--vscode-button-secondaryBackground);
      color: var(--vscode-button-secondaryForeground);
    }
    .btn-secondary:hover {
      background: var(--vscode-button-secondaryHoverBackground);
    }
    .btn-icon {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 22px;
      height: 22px;
      background: transparent;
      border: none;
      cursor: pointer;
      color: var(--vscode-icon-foreground);
      border-radius: 2px;
    }
    .btn-icon:hover {
      background: var(--vscode-toolbar-hoverBackground);
    }
    .btn-icon.btn-danger:hover {
      color: var(--vscode-errorForeground);
    }

    /* Form */
    .form-header {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 12px;
      border-bottom: 1px solid var(--vscode-panel-border);
      flex-shrink: 0;
    }
    .form-header span {
      font-weight: 500;
    }
    .form-body {
      padding: 12px;
      overflow-y: auto;
      flex: 1;
    }
    .form-group {
      display: flex;
      flex-direction: column;
      gap: 4px;
      margin-bottom: 12px;
    }
    .form-row {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 8px;
    }
    label {
      font-size: 11px;
      color: var(--vscode-descriptionForeground);
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }
    input[type="text"],
    input[type="number"],
    input[type="password"] {
      padding: 4px 8px;
      background: var(--vscode-input-background);
      color: var(--vscode-input-foreground);
      border: 1px solid var(--vscode-input-border, transparent);
      outline: none;
      font-size: var(--vscode-font-size);
      font-family: var(--vscode-font-family);
      width: 100%;
    }
    input:focus {
      border-color: var(--vscode-focusBorder);
    }
    .checkbox-row {
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .checkbox-row label {
      text-transform: none;
      letter-spacing: normal;
      font-size: var(--vscode-font-size);
      color: var(--vscode-foreground);
      cursor: pointer;
    }
    .form-actions {
      display: flex;
      align-items: center;
      gap: 8px;
      padding-top: 12px;
      border-top: 1px solid var(--vscode-panel-border);
      margin-top: 4px;
      flex-wrap: wrap;
    }
    .test-status {
      font-size: 11px;
      flex: 1;
    }
    .test-status.success { color: var(--vscode-testing-iconPassed, #73c991); }
    .test-status.error   { color: var(--vscode-errorForeground); }
    .test-status.testing { color: var(--vscode-descriptionForeground); }

    /* Placeholder panels */
    .placeholder {
      display: flex;
      align-items: center;
      justify-content: center;
      flex: 1;
      color: var(--vscode-descriptionForeground);
      font-style: italic;
      padding: 24px;
    }
  </style>
</head>
<body>

  <!-- Tab bar -->
  <div class="tab-bar">
    <button class="tab" data-tab="explorer">
      <i class="codicon codicon-database"></i>Explorer
    </button>
    <button class="tab" data-tab="query">
      <i class="codicon codicon-file-code"></i>Query
    </button>
    <button class="tab" data-tab="history">
      <i class="codicon codicon-history"></i>History
    </button>
  </div>

  <!-- Explorer panel -->
  <div class="panel" id="panel-explorer">

    <!-- List view -->
    <div class="view" id="view-list">
      <div class="empty-state" id="empty-state">
        <span>No connections yet.</span>
        <button class="btn btn-primary" id="btn-add-first">
          <i class="codicon codicon-add"></i>Add Connection
        </button>
      </div>
      <div class="conn-list hidden" id="conn-list"></div>
      <div class="list-footer hidden" id="list-footer">
        <button class="btn btn-secondary" id="btn-add-conn">
          <i class="codicon codicon-add"></i>Add Connection
        </button>
      </div>
    </div>

    <!-- Form view -->
    <div class="view hidden" id="view-form">
      <div class="form-header">
        <button class="btn-icon" id="btn-back" title="Back">
          <i class="codicon codicon-arrow-left"></i>
        </button>
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
    const vscode = acquireVsCodeApi();

    // Persisted state (survives hide/show)
    let persisted = vscode.getState() || { tab: 'explorer' };
    let connections = [];
    let editingId = null;

    // Elements
    const tabs      = document.querySelectorAll('.tab');
    const panels    = document.querySelectorAll('.panel');
    const viewList  = document.getElementById('view-list');
    const viewForm  = document.getElementById('view-form');
    const emptyState = document.getElementById('empty-state');
    const connList  = document.getElementById('conn-list');
    const listFooter = document.getElementById('list-footer');
    const formTitle = document.getElementById('form-title');
    const testStatus = document.getElementById('test-status');

    function saveState() { vscode.setState(persisted); }

    // --- Tab switching ---
    function switchTab(tab) {
      persisted.tab = tab;
      saveState();
      tabs.forEach(function (b) { b.classList.toggle('active', b.dataset.tab === tab); });
      panels.forEach(function (p) { p.classList.toggle('hidden', p.id !== 'panel-' + tab); });
    }

    tabs.forEach(function (btn) {
      btn.addEventListener('click', function () { switchTab(btn.dataset.tab); });
    });

    // --- Escape helper (XSS guard) ---
    function esc(str) {
      return String(str)
        .replace(/&/g, '&amp;')
        .replace(/\x3c/g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
    }

    // --- Render connection list ---
    function renderConnections() {
      if (connections.length === 0) {
        emptyState.classList.remove('hidden');
        connList.classList.add('hidden');
        listFooter.classList.add('hidden');
      } else {
        emptyState.classList.add('hidden');
        connList.classList.remove('hidden');
        listFooter.classList.remove('hidden');
        connList.innerHTML = connections.map(function (c) {
          return '<div class="conn-item" data-id="' + esc(c.id) + '">'
            + '<i class="codicon codicon-database"></i>'
            + '<div class="conn-info">'
            +   '<span class="conn-label">' + esc(c.label) + '</span>'
            +   '<span class="conn-meta">' + esc(c.user) + '@' + esc(c.host) + ':' + esc(String(c.port)) + '/' + esc(c.database) + '</span>'
            + '</div>'
            + '<div class="conn-actions">'
            +   '<button class="btn-icon" title="Edit" data-action="edit" data-id="' + esc(c.id) + '"><i class="codicon codicon-edit"></i></button>'
            +   '<button class="btn-icon btn-danger" title="Delete" data-action="delete" data-id="' + esc(c.id) + '"><i class="codicon codicon-trash"></i></button>'
            + '</div>'
            + '</div>';
        }).join('');
      }
    }

    // --- Form helpers ---
    function showForm(conn) {
      editingId = conn ? conn.id : null;
      formTitle.textContent = conn ? 'Edit Connection' : 'Add Connection';
      document.getElementById('f-label').value    = conn ? conn.label    : '';
      document.getElementById('f-host').value     = conn ? conn.host     : 'localhost';
      document.getElementById('f-port').value     = conn ? String(conn.port) : '5432';
      document.getElementById('f-database').value = conn ? conn.database : '';
      document.getElementById('f-user').value     = conn ? conn.user     : '';
      document.getElementById('f-password').value = '';
      document.getElementById('f-ssl').checked    = conn ? conn.ssl      : false;
      testStatus.textContent = '';
      testStatus.className   = 'test-status';
      viewList.classList.add('hidden');
      viewForm.classList.remove('hidden');
    }

    function showList() {
      editingId = null;
      viewForm.classList.add('hidden');
      viewList.classList.remove('hidden');
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

    // --- Button listeners ---
    document.getElementById('btn-add-first').addEventListener('click', function () { showForm(null); });
    document.getElementById('btn-add-conn').addEventListener('click',  function () { showForm(null); });
    document.getElementById('btn-back').addEventListener('click', showList);

    document.getElementById('btn-test').addEventListener('click', function () {
      testStatus.textContent = 'Testing...';
      testStatus.className   = 'test-status testing';
      var d = getFormData();
      vscode.postMessage({ command: 'testConnection', data: d });
    });

    document.getElementById('conn-form').addEventListener('submit', function (e) {
      e.preventDefault();
      vscode.postMessage({ command: 'saveConnection', data: getFormData() });
    });

    connList.addEventListener('click', function (e) {
      var btn = e.target.closest('[data-action]');
      if (!btn) { return; }
      var id     = btn.dataset.id;
      var action = btn.dataset.action;
      if (action === 'edit') {
        var conn = connections.find(function (c) { return c.id === id; });
        if (conn) { showForm(conn); }
      } else if (action === 'delete') {
        vscode.postMessage({ command: 'deleteConnection', data: { id: id } });
      }
    });

    // --- Messages from extension host ---
    window.addEventListener('message', function (event) {
      var msg = event.data;
      switch (msg.command) {
        case 'updateConnections':
          connections = msg.data || [];
          renderConnections();
          if (!viewForm.classList.contains('hidden')) { showList(); }
          break;
        case 'testResult':
          if (msg.data.success) {
            testStatus.textContent = 'Connected successfully!';
            testStatus.className   = 'test-status success';
          } else {
            testStatus.textContent = msg.data.message || 'Connection failed';
            testStatus.className   = 'test-status error';
          }
          break;
      }
    });

    // --- Init ---
    switchTab(persisted.tab || 'explorer');
    vscode.postMessage({ command: 'ready' });
  }());
  </script>
</body>
</html>`;
}
