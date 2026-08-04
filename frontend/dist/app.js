const invoke = window.__TAURI__?.core?.invoke;
const currentWindow = window.__TAURI__?.window?.getCurrentWindow?.();
const state = { connections: [], selectedConnectionId: null, selectedSchema: null, explorerFilter: '', explorerFilterRequest: 0, lastTested: null, result: null, currentQueryId: null, currentQueryOperationId: null, cancelRequested: false, currentOperationId: null, queryTabs: [{ id: 1, label: 'Query 1', sql: '' }], currentQueryTabId: 1 };
let dialogResolver = null;

const byId = (id) => document.getElementById(id);
const value = (id) => byId(id).value.trim();
const optional = (id) => value(id) || null;
const numberOrNull = (id) => value(id) ? Number(value(id)) : null;

function saveCurrentQueryTab() {
  const tab = state.queryTabs.find((item) => item.id === state.currentQueryTabId);
  if (tab) tab.sql = byId('sql-editor').value;
}

function renderQueryTabs() {
  const tabs = byId('query-tabs');
  if (!tabs) return;
  tabs.replaceChildren();
  for (const tab of state.queryTabs) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'query-tab';
    button.role = 'tab';
    button.ariaSelected = String(tab.id === state.currentQueryTabId);
    button.textContent = tab.label;
    button.addEventListener('click', () => selectQueryTab(tab.id));
    tabs.append(button);
  }
  const add = document.createElement('button');
  add.type = 'button';
  add.className = 'query-tab-new';
  add.textContent = '+';
  add.title = 'New query';
  add.setAttribute('aria-label', 'New query');
  add.addEventListener('click', newQueryTab);
  tabs.append(add);
}

function selectQueryTab(id) {
  saveCurrentQueryTab();
  const tab = state.queryTabs.find((item) => item.id === id);
  if (!tab) return;
  state.currentQueryTabId = id;
  byId('sql-editor').value = tab.sql;
  renderQueryTabs();
  byId('sql-editor').focus();
}

function newQueryTab() {
  saveCurrentQueryTab();
  const next = Math.max(...state.queryTabs.map((item) => item.id), 0) + 1;
  state.queryTabs.push({ id: next, label: `Query ${next}`, sql: '' });
  state.currentQueryTabId = next;
  byId('sql-editor').value = '';
  renderQueryTabs();
  byId('sql-editor').focus();
}

async function saveCurrentSnippet() {
  const name = await showPrompt('Choose a name for this reusable query.', 'Save snippet', 'Snippet name', 'e.g. active users');
  const sql = byId('sql-editor').value;
  if (!name || !sql.trim()) return;
  try {
    await invoke('save_snippet', { input: { name, sql, conn_id: byId('query-connection').value || null } });
    byId('query-status').textContent = 'Snippet saved';
  } catch {
    byId('query-status').textContent = 'Could not save snippet';
  }
}

function selectedQueryText() {
  const editor = byId('sql-editor');
  const selected = editor.value.slice(editor.selectionStart, editor.selectionEnd).trim();
  return selected || editor.value;
}

function formatSql(sql) {
  const keywords = /\b(SELECT|FROM|WHERE|JOIN|LEFT JOIN|RIGHT JOIN|INNER JOIN|GROUP BY|ORDER BY|HAVING|LIMIT|OFFSET|RETURNING|VALUES|SET|UNION ALL|UNION)\b/gi;
  return sql
    .trim()
    .replace(/[ \t\r\n]+/g, ' ')
    .replace(/\s*,\s*/g, ', ')
    .replace(/\s+(LEFT JOIN|RIGHT JOIN|INNER JOIN|JOIN|FROM|WHERE|GROUP BY|ORDER BY|HAVING|LIMIT|OFFSET|RETURNING|VALUES|SET|UNION ALL|UNION)\s+/gi, '\n$1 ')
    .replace(keywords, (keyword) => keyword.toUpperCase())
    .replace(/\n{2,}/g, '\n')
    .trim();
}

function setStatus(message, kind = '') {
  const element = byId('form-status');
  element.textContent = message;
  element.className = `form-status ${kind}`;
}

function stateClass(connection) {
  return connection.state === 'connected' ? 'connected' : connection.state === 'connecting' ? 'connecting' : connection.state === 'error' ? 'error' : '';
}

function connectionInput() {
  return {
    id: optional('connection-id'),
    label: value('label'),
    host: value('host'),
    port: Number(value('port')),
    database: value('database'),
    user: value('user'),
    ssl: byId('ssl').checked,
    ssh_enabled: byId('ssh-enabled').checked,
    ssh_host: optional('ssh-host'),
    ssh_port: numberOrNull('ssh-port'),
    ssh_user: optional('ssh-user'),
    ssh_key_path: optional('ssh-key-path'),
    ssh_jump_host: optional('jump-host'),
    ssh_jump_port: numberOrNull('jump-port'),
    ssh_jump_user: optional('jump-user'),
    ssh_jump_key_path: optional('jump-key-path'),
    favorite: byId('favorite').checked,
  };
}

function connectionRequest() {
  return {
    input: connectionInput(),
    password: optional('password'),
    ssh_password: optional('ssh-password'),
    jump_password: optional('jump-password'),
  };
}

function clearCredentials() {
  byId('password').value = '';
  byId('ssh-password').value = '';
  byId('jump-password').value = '';
}

function renderConnections() {
  const list = byId('connection-list');
  list.replaceChildren();
  byId('connection-count').textContent = `${state.connections.length} connection${state.connections.length === 1 ? '' : 's'}`;
  if (!state.connections.length) {
    const empty = document.createElement('div');
    empty.className = 'empty-state';
    const title = document.createElement('strong');
    title.textContent = 'No saved connections';
    const hint = document.createElement('span');
    hint.textContent = 'Add a connection to start exploring PostgreSQL.';
    empty.append(title, hint);
    list.append(empty);
    return;
  }
  const connections = [...state.connections].sort((left, right) => Number(right.favorite) - Number(left.favorite));
  for (const connection of connections) {
    const card = document.createElement('article');
    card.className = 'connection-card';
    const meta = document.createElement('div');
    meta.className = 'connection-meta';
    const dot = document.createElement('span');
    dot.className = `state-dot ${stateClass(connection)}`;
    const title = document.createElement('strong');
    title.textContent = connection.label;
    const status = document.createElement('small');
    status.textContent = connection.state;
    if (connection.favorite) {
      const favorite = document.createElement('span');
      favorite.className = 'favorite-mark';
      favorite.textContent = '★';
      favorite.title = 'Favorite connection';
      favorite.setAttribute('aria-label', 'Favorite connection');
      meta.append(dot, title, favorite, status);
    } else {
      meta.append(dot, title, status);
    }
    const detail = document.createElement('div');
    detail.className = 'connection-detail';
    detail.textContent = `${connection.user}@${connection.host}:${connection.port}/${connection.database}${connection.ssh_enabled ? ' · SSH' : ''}`;
    const actions = document.createElement('div');
    actions.className = 'connection-actions';
    const connectButton = document.createElement('button');
    connectButton.className = `button small ${connection.state === 'connected' ? 'disconnect-action' : 'connect-action'}`;
    connectButton.type = 'button';
    connectButton.textContent = connection.state === 'connected' ? 'Disconnect' : 'Connect';
    connectButton.addEventListener('click', () => connection.state === 'connected' ? disconnect(connection.id) : connect(connection.id));
    const edit = document.createElement('button');
    edit.className = 'button small';
    edit.type = 'button';
    edit.textContent = 'Edit';
    edit.addEventListener('click', () => showForm(connection));
    const remove = document.createElement('button');
    remove.className = 'button small danger';
    remove.type = 'button';
    remove.textContent = 'Delete';
    remove.addEventListener('click', () => deleteConnection(connection.id));
    actions.append(connectButton, edit, remove);
    card.append(meta, actions, detail);
    list.append(card);
  }
}

function showForm(connection = null) {
  byId('connection-form').hidden = false;
  byId('form-title').textContent = connection ? 'Edit connection' : 'New connection';
  byId('connection-id').value = connection?.id ?? '';
  byId('label').value = connection?.label ?? '';
  byId('host').value = connection?.host ?? 'localhost';
  byId('port').value = connection?.port ?? 5432;
  byId('database').value = connection?.database ?? '';
  byId('user').value = connection?.user ?? '';
  byId('ssl').checked = connection?.ssl ?? false;
  byId('favorite').checked = connection?.favorite ?? false;
  byId('ssh-enabled').checked = connection?.ssh_enabled ?? false;
  byId('ssh-host').value = connection?.ssh_host ?? '';
  byId('ssh-port').value = connection?.ssh_port ?? 22;
  byId('ssh-user').value = connection?.ssh_user ?? '';
  byId('ssh-key-path').value = connection?.ssh_key_path ?? '';
  byId('jump-host').value = connection?.ssh_jump_host ?? '';
  byId('jump-port').value = connection?.ssh_jump_port ?? 22;
  byId('jump-user').value = connection?.ssh_jump_user ?? '';
  byId('jump-key-path').value = connection?.ssh_jump_key_path ?? '';
  clearCredentials();
  setStatus(connection ? 'Existing credentials stay in the Secret Service when left blank.' : 'Test the connection before saving.', '');
  byId('label').focus();
}

function hideForm() {
  byId('connection-form').hidden = true;
  byId('connection-form').reset();
  clearCredentials();
  state.lastTested = null;
}

async function refreshConnections() {
  state.connections = await invoke('list_connections');
  renderConnections();
  renderExplorerConnections();
  renderQueryConnections();
  renderAdvancedConnections();
}

async function connect(id) {
  const connection = state.connections.find((item) => item.id === id);
  if (!connection) return;
  connection.state = 'connecting';
  renderConnections();
  try {
    const updated = await invoke('connect_stored', { id, statementTimeoutMs: 30000 });
    state.connections = state.connections.map((item) => item.id === id ? updated : item);
    state.selectedConnectionId = id;
    renderConnections();
    renderExplorerConnections();
  } catch (error) {
    connection.state = 'error';
    connection.error = 'Connection failed';
    renderConnections();
  }
}

async function disconnect(id) {
  try {
    await invoke('disconnect', { id });
    await refreshConnections();
  } catch (error) {
    setStatus('Could not disconnect the connection.', 'error');
  }
}

async function deleteConnection(id) {
  if (!await showConfirm('Delete this connection and its stored credentials?', 'Delete connection', true)) return;
  try {
    await invoke('delete_connection', { id });
    if (state.selectedConnectionId === id) state.selectedConnectionId = null;
    await refreshConnections();
  } catch (error) {
    setStatus('Could not delete the connection.', 'error');
  }
}

async function testCurrentConnection() {
  const request = connectionRequest();
  setStatus('Testing connection…');
  byId('test-connection').disabled = true;
  try {
    await invoke('test_connection', { request });
    state.lastTested = JSON.stringify(request.input);
    setStatus('Connection successful.', 'success');
    return true;
  } catch (error) {
    state.lastTested = null;
    setStatus('Connection failed. Check the fields and credentials.', 'error');
    return false;
  } finally {
    byId('test-connection').disabled = false;
  }
}

async function saveCurrentConnection(event) {
  event.preventDefault();
  const request = connectionRequest();
  if (!(await testCurrentConnection())) return;
  setStatus('Saving connection…');
  try {
    await invoke('save_connection', { request });
    hideForm();
    await refreshConnections();
  } catch (error) {
    setStatus('Could not save the connection.', 'error');
  }
}

function renderExplorerConnections() {
  const pane = byId('explorer-connections');
  pane.replaceChildren();
  for (const connection of state.connections) {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = `tree-item ${state.selectedConnectionId === connection.id ? 'selected' : ''}`;
    item.textContent = connection.label;
    item.addEventListener('click', () => openExplorer(connection.id));
    pane.append(item);
  }
  if (!state.connections.length) {
    const empty = document.createElement('div');
    empty.className = 'empty-state';
    empty.textContent = 'No connections';
    pane.append(empty);
  }
}

function renderQueryConnections() {
  const select = byId('query-connection');
  const selected = select.value;
  select.replaceChildren();
  const placeholder = document.createElement('option');
  placeholder.value = '';
  placeholder.textContent = 'Select a connected connection';
  select.append(placeholder);
  for (const connection of state.connections.filter((item) => item.state === 'connected')) {
    const option = document.createElement('option');
    option.value = connection.id;
    option.textContent = connection.label;
    select.append(option);
  }
  select.value = state.connections.some((item) => item.id === selected && item.state === 'connected') ? selected : '';
}

function renderAdvancedConnections() {
  for (const id of ['dashboard-connection', 'admin-connection', 'backup-connection', 'assistant-connection']) {
    const select = byId(id);
    const selected = select.value;
    select.replaceChildren();
    const placeholder = document.createElement('option'); placeholder.value = ''; placeholder.textContent = 'Select a connected connection'; select.append(placeholder);
    for (const connection of state.connections.filter((item) => item.state === 'connected')) {
      const option = document.createElement('option'); option.value = connection.id; option.textContent = connection.label; select.append(option);
    }
    select.value = state.connections.some((item) => item.id === selected && item.state === 'connected') ? selected : '';
  }
}

function renderAssistantHistory(history) {
  const transcript = byId('assistant-history'); transcript.replaceChildren();
  if (!history.length) { transcript.append(errorState('Ask about a query, schema, or execution plan', 'The assistant can inspect the connected database but never changes it.')); return; }
  for (const message of history) {
    const bubble = document.createElement('article'); bubble.className = `assistant-message ${message.role === 'user' ? 'user' : ''}`;
    const label = document.createElement('small'); label.textContent = message.tool_label ? `Tool · ${message.tool_label}` : message.role === 'user' ? 'You' : 'Assistant';
    const content = document.createElement('div'); content.textContent = message.content; bubble.append(label, content); transcript.append(bubble);
  }
  transcript.scrollTop = transcript.scrollHeight;
}

async function loadAssistant(id) {
  if (!id) { renderAssistantHistory([]); return; }
  try { renderAssistantHistory(await invoke('assistant_history', { id })); } catch (error) { renderAssistantHistory([]); byId('assistant-status').textContent = 'Could not load assistant history.'; }
}

async function sendAssistant() {
  const id = byId('assistant-connection').value; const message = byId('assistant-message').value;
  if (!id) { byId('assistant-status').textContent = 'Select a connected connection'; return; }
  if (!message.trim()) return;
  byId('send-assistant').disabled = true; byId('assistant-status').textContent = 'Thinking…';
  try {
    const reply = await invoke('assistant_send', { id, message });
    renderAssistantHistory(reply.history); byId('assistant-message').value = ''; byId('assistant-status').textContent = `${reply.input_tokens} input · ${reply.output_tokens} output tokens`;
  } catch (error) { byId('assistant-status').textContent = 'Assistant request failed. Configure a provider key and reconnect.'; }
  finally { byId('send-assistant').disabled = false; }
}

function operationId() { return `ui-${Date.now()}-${Math.random().toString(36).slice(2)}`; }

async function runBackup(restore = false) {
  const id = byId('backup-connection').value; if (!id) { byId('backup-status').textContent = 'Select a connected connection'; return; }
  if (restore && !await showConfirm('Restore will write data into the selected database. Continue only if the input and target are correct.', 'Confirm restore', true)) return;
  const operation = operationId(); state.currentOperationId = operation; byId('cancel-operation').hidden = false; byId('backup-status').textContent = restore ? 'Restoring…' : 'Creating backup…'; byId('backup-log').textContent = '';
  for (const control of ['run-backup', 'run-restore', 'backup-connection', 'backup-output', 'backup-format', 'restore-input', 'restore-database']) byId(control).disabled = true;
  const options = restore ? { input: value('restore-input'), target_database: value('restore-database'), clean: false, single_transaction: true } : { output: value('backup-output'), format: byId('backup-format').value, compression: null, schemas: [], tables: [] };
  try {
    const result = await invoke(restore ? 'run_restore' : 'run_backup', { id, operationId: operation, options });
    byId('backup-log').textContent = result.logs.join('\n'); byId('backup-status').textContent = result.cancelled ? 'Cancelled' : result.succeeded ? 'Completed' : `Failed (exit ${result.exit_code ?? 'unknown'})`;
  } catch (error) { byId('backup-status').textContent = 'Backup operation failed. Check the path and connection.'; }
  finally {
    state.currentOperationId = null; byId('cancel-operation').hidden = true;
    for (const control of ['run-backup', 'run-restore', 'backup-connection', 'backup-output', 'backup-format', 'restore-input', 'restore-database']) byId(control).disabled = false;
  }
}

async function cancelOperation() { if (!state.currentOperationId) return; byId('backup-status').textContent = 'Cancelling…'; try { await invoke('cancel_operation', { operationId: state.currentOperationId }); } catch (error) {} }

function metric(label, value) {
  const card = document.createElement('div'); card.className = 'metric-card';
  const caption = document.createElement('span'); caption.className = 'eyebrow'; caption.textContent = label;
  const number = document.createElement('strong'); number.textContent = value ?? '—';
  card.append(caption, number); return card;
}

async function loadDashboard(id) {
  const content = byId('dashboard-content'); content.replaceChildren();
  if (!id) { const empty = document.createElement('div'); empty.className = 'empty-state'; empty.textContent = 'Choose a connected connection'; content.append(empty); return; }
  const loading = document.createElement('div'); loading.className = 'empty-state'; loading.textContent = 'Loading dashboard…'; content.append(loading);
  try {
    const payload = await invoke('dashboard', { id });
    const data = payload.dashboard; const stats = payload.stats;
    content.replaceChildren();
    const metrics = document.createElement('div'); metrics.className = 'metric-grid';
    for (const [label, key] of [['PostgreSQL', 'pg_version'], ['Database size', 'db_size'], ['Cache hit', 'cache_hit'], ['Active connections', 'active_conn'], ['Transactions', 'commits'], ['Rollbacks', 'rollbacks']]) metrics.append(metric(label, data[key]));
    content.append(metrics);
    const info = document.createElement('div'); info.className = 'advanced-grid';
    info.append(dataPanel('Database', [['Name', data.db_name], ['Encoding', data.encoding], ['Collation', data.collation], ['Uptime', data.uptime]]));
    info.append(dataPanel('Largest tables', (data.top_tables || []).map((row) => [`${row.schema}.${row.table}`, row.total_size])));
    info.append(dataPanel('Statistics', [['Size', stats.db?.size], ['Cache hit', stats.db?.cache_hit_pct], ['Deadlocks', data.deadlocks], ['Temp files', data.temp_files]]));
    content.append(info);
  } catch (error) { content.replaceChildren(errorState('Dashboard unavailable', 'Check permissions and reconnect.')); }
}

function dataPanel(title, rows) {
  const panel = document.createElement('div'); panel.className = 'data-panel'; const heading = document.createElement('h3'); heading.textContent = title; panel.append(heading);
  for (const [label, value] of rows.slice(0, 12)) { const row = document.createElement('div'); row.className = 'data-row'; const key = document.createElement('span'); key.textContent = label; const val = document.createElement('strong'); val.textContent = value ?? '—'; row.append(key, val); panel.append(row); }
  return panel;
}

function errorState(title, message) { const empty = document.createElement('div'); empty.className = 'empty-state'; const strong = document.createElement('strong'); strong.textContent = title; const span = document.createElement('span'); span.textContent = message; empty.append(strong, span); return empty; }

async function loadAdmin(id) {
  const content = byId('admin-content'); content.replaceChildren();
  if (!id) { content.append(errorState('Choose a connected connection', 'Activity and locks are read from PostgreSQL.')); return; }
  content.append(errorState('Loading administration…', ''));
  try {
    const payload = await invoke('admin', { id }); content.replaceChildren();
    content.append(dataPanel('Activity', (payload.activity || []).slice(0, 12).map((row) => [`${row.pid} · ${row.state || 'unknown'}`, row.query || 'idle'])));
    content.append(dataPanel('Locks', (payload.locks || []).slice(0, 12).map((row) => [`${row.blocked_pid} → ${row.blocking_pid}`, row.wait_sec ? `${row.wait_sec}s` : 'waiting'])));
    const installed = payload.extensions?.installed || []; content.append(dataPanel('Extensions', installed.slice(0, 12).map((row) => [row.name, row.installed_version || 'installed'])));
    const queryStats = payload.query_stats?.queries || []; content.append(dataPanel('Query stats', queryStats.slice(0, 12).map((row) => [row.query, `${row.total_exec_ms.toFixed?.(1) ?? row.total_exec_ms} ms`])));
  } catch (error) { content.replaceChildren(errorState('Administration unavailable', 'Check permissions and reconnect.')); }
}

async function openTable(id, schema, table) {
  switchView('table-detail'); byId('detail-title').textContent = table; byId('detail-eyebrow').textContent = `${schema.toUpperCase()} · TABLE DETAIL`; byId('detail-summary').textContent = 'Loading';
  const content = byId('detail-content'); content.replaceChildren(errorState('Loading table detail…', ''));
  try {
    const payload = await invoke('table_detail', { id, schema, table }); const detail = payload.detail; content.replaceChildren(); byId('detail-summary').textContent = `${detail.row_estimate ?? 0} estimated rows`;
    content.append(dataPanel('Columns', (detail.columns || []).map((row) => [row.name, `${row.full_type || row.data_type}${row.is_nullable ? '' : ' · NOT NULL'}${row.is_primary_key ? ' · PK' : ''}`])));
    content.append(dataPanel('Constraints', (detail.constraints || []).map((row) => [row.name, `${row.type}: ${row.definition}`])));
    content.append(dataPanel('Indexes', (detail.indexes || []).map((row) => [row.name, row.definition])));
    content.append(dataPanel('Foreign keys', (detail.fk_map || []).map((row) => [row.constraint_name, `${row.direction} · ${row.foreign_table}.${row.foreign_column}`])));
  } catch (error) { content.replaceChildren(errorState('Table detail unavailable', 'The object may have been removed or permission denied.')); }
}

async function openErd() {
  const id = state.selectedConnectionId; const schema = state.selectedSchema; if (!id || !schema) return;
  switchView('erd'); byId('erd-title').textContent = `ERD · ${schema}`; byId('erd-summary').textContent = 'Loading';
  const content = byId('erd-content'); content.replaceChildren(errorState('Loading ERD…', ''));
  try {
    const payload = await invoke('erd', { id, schema }); const data = payload.data; content.replaceChildren(); byId('erd-summary').textContent = `${(data.tables || []).length} tables · ${(data.relations || []).length} relations`;
    content.append(renderErdCanvas(data));
    const relationsPanel = dataPanel('Relations', (data.relations || []).map((row) => [`${row.from_table}.${row.from_column}`, `→ ${row.to_table}.${row.to_column}`]));
    relationsPanel.classList.add('erd-relations-panel');
    content.append(relationsPanel);
  } catch (error) { content.replaceChildren(errorState('ERD unavailable', 'Check schema permissions and reconnect.')); }
}

function renderErdCanvas(data) {
  const tables = data.tables || [];
  const relations = data.relations || [];
  const columns = Math.max(1, Math.ceil(Math.sqrt(tables.length)));
  const cardWidth = 230;
  const cardHeight = 190;
  const gapX = 34;
  const gapY = 34;
  const width = Math.max(720, columns * (cardWidth + gapX) + 40);
  const rows = Math.max(1, Math.ceil(tables.length / columns));
  const height = Math.max(420, rows * (cardHeight + gapY) + 40);
  const positions = new Map(tables.map((table, index) => [table.name, {
    x: 20 + (index % columns) * (cardWidth + gapX),
    y: 20 + Math.floor(index / columns) * (cardHeight + gapY),
  }]));
  let zoom = 1;
  let panX = 0;
  let panY = 0;
  let selectedTable = null;
  const shell = document.createElement('section'); shell.className = 'erd-workspace';
  const toolbar = document.createElement('div'); toolbar.className = 'erd-toolbar';
  const hint = document.createElement('span'); hint.className = 'query-hint'; hint.textContent = 'Drag to pan · Click a table to inspect it';
  const controls = document.createElement('div'); controls.className = 'erd-controls';
  const zoomLabel = document.createElement('span'); zoomLabel.className = 'badge';
  const zoomOut = document.createElement('button'); zoomOut.className = 'button small'; zoomOut.type = 'button'; zoomOut.textContent = '−'; zoomOut.title = 'Zoom out';
  const zoomIn = document.createElement('button'); zoomIn.className = 'button small'; zoomIn.type = 'button'; zoomIn.textContent = '+'; zoomIn.title = 'Zoom in';
  const reset = document.createElement('button'); reset.className = 'button small'; reset.type = 'button'; reset.textContent = 'Reset view';
  controls.append(zoomOut, zoomLabel, zoomIn, reset); toolbar.append(hint, controls);
  const canvas = document.createElement('div'); canvas.className = 'erd-canvas'; canvas.tabIndex = 0; canvas.setAttribute('aria-label', 'Entity relationship diagram');
  const viewport = document.createElement('div'); viewport.className = 'erd-viewport'; viewport.style.width = `${width}px`; viewport.style.height = `${height}px`;
  const links = document.createElementNS('http://www.w3.org/2000/svg', 'svg'); links.classList.add('erd-links'); links.setAttribute('width', width); links.setAttribute('height', height); links.setAttribute('viewBox', `0 0 ${width} ${height}`);
  const defs = document.createElementNS('http://www.w3.org/2000/svg', 'defs');
  const marker = document.createElementNS('http://www.w3.org/2000/svg', 'marker'); marker.setAttribute('id', 'erd-arrow'); marker.setAttribute('markerWidth', '8'); marker.setAttribute('markerHeight', '8'); marker.setAttribute('refX', '7'); marker.setAttribute('refY', '3'); marker.setAttribute('orient', 'auto');
  const arrow = document.createElementNS('http://www.w3.org/2000/svg', 'path'); arrow.setAttribute('d', 'M0,0 L0,6 L7,3 z'); arrow.setAttribute('fill', '#d96558'); marker.append(arrow); defs.append(marker); links.append(defs);
  for (const relation of relations) {
    const from = positions.get(relation.from_table); const to = positions.get(relation.to_table);
    if (!from || !to) continue;
    const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
    line.setAttribute('x1', from.x + cardWidth / 2); line.setAttribute('y1', from.y + cardHeight / 2); line.setAttribute('x2', to.x + cardWidth / 2); line.setAttribute('y2', to.y + cardHeight / 2); line.setAttribute('marker-end', 'url(#erd-arrow)'); line.dataset.from = relation.from_table; line.dataset.to = relation.to_table; links.append(line);
  }
  const nodes = document.createElement('div'); nodes.className = 'erd-nodes';
  const updateSelection = () => {
    for (const node of nodes.querySelectorAll('.erd-node')) node.classList.toggle('selected', node.dataset.table === selectedTable);
    for (const line of links.querySelectorAll('line')) line.classList.toggle('selected', line.dataset.from === selectedTable || line.dataset.to === selectedTable);
  };
  for (const table of tables) {
    const node = document.createElement('article'); node.className = 'erd-node'; node.dataset.table = table.name; node.tabIndex = 0; node.setAttribute('role', 'button'); node.style.left = `${positions.get(table.name).x}px`; node.style.top = `${positions.get(table.name).y}px`;
    const header = document.createElement('div'); header.className = 'erd-node-header';
    const title = document.createElement('strong'); title.textContent = table.name;
    const kind = document.createElement('span'); kind.textContent = table.kind === 'view' ? 'VIEW' : 'TABLE'; header.append(title, kind);
    const list = document.createElement('div'); list.className = 'erd-node-columns';
    for (const column of (table.columns || []).slice(0, 8)) { const row = document.createElement('div'); row.className = 'erd-node-column'; const name = document.createElement('span'); name.textContent = `${column.is_pk ? '◆ ' : column.is_fk ? '↳ ' : ''}${column.name}`; const type = document.createElement('small'); type.textContent = column.data_type; row.append(name, type); list.append(row); }
    if ((table.columns || []).length > 8) { const more = document.createElement('small'); more.className = 'erd-node-more'; more.textContent = `+${table.columns.length - 8} more columns`; list.append(more); }
    node.append(header, list);
    const select = () => { selectedTable = table.name; updateSelection(); byId('erd-summary').textContent = `${table.name} · ${(table.columns || []).length} columns`; };
    node.addEventListener('click', select); node.addEventListener('keydown', (event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(); } });
    nodes.append(node);
  }
  viewport.append(links, nodes); canvas.append(viewport); shell.append(toolbar, canvas);
  const updateTransform = () => { viewport.style.transform = `translate(${panX}px, ${panY}px) scale(${zoom})`; zoomLabel.textContent = `${Math.round(zoom * 100)}%`; };
  const changeZoom = (amount) => { zoom = Math.min(1.8, Math.max(.5, Number((zoom + amount).toFixed(2)))); updateTransform(); };
  zoomOut.addEventListener('click', () => changeZoom(-.1)); zoomIn.addEventListener('click', () => changeZoom(.1)); reset.addEventListener('click', () => { zoom = 1; panX = 0; panY = 0; updateTransform(); });
  let dragging = false; let lastX = 0; let lastY = 0;
  canvas.addEventListener('pointerdown', (event) => { if (event.target.closest('.erd-node, button')) return; dragging = true; lastX = event.clientX; lastY = event.clientY; canvas.setPointerCapture(event.pointerId); });
  canvas.addEventListener('pointermove', (event) => { if (!dragging) return; panX += event.clientX - lastX; panY += event.clientY - lastY; lastX = event.clientX; lastY = event.clientY; updateTransform(); });
  canvas.addEventListener('pointerup', () => { dragging = false; });
  updateTransform();
  return shell;
}

function renderResult(result) {
  state.result = result;
  const grid = byId('result-grid');
  const error = byId('result-error');
  document.querySelector('.result-note')?.remove();
  error.textContent = '';
  byId('export-csv').disabled = !result;
  byId('export-json').disabled = !result;
  if (!result) {
    byId('result-summary').textContent = 'No result';
    grid.replaceChildren();
    const empty = document.createElement('div');
    empty.className = 'empty-state';
    empty.textContent = 'Run a query to see results';
    grid.append(empty);
    return;
  }
  byId('result-summary').textContent = `${result.rows.length} rows · ${result.duration_ms} ms`;
  grid.replaceChildren();
  if (!result.columns.length) {
    const empty = document.createElement('div');
    empty.className = 'empty-state';
    empty.textContent = 'Statement completed without rows';
    grid.append(empty);
    return;
  }
  const table = document.createElement('table');
  const head = document.createElement('thead');
  const headerRow = document.createElement('tr');
  for (const column of result.columns) { const cell = document.createElement('th'); cell.textContent = column; headerRow.append(cell); }
  head.append(headerRow);
  const body = document.createElement('tbody');
  const visibleRows = result.rows.slice(0, 500);
  for (const row of visibleRows) {
    const tr = document.createElement('tr');
    for (const column of result.columns) {
      const cell = document.createElement('td');
      const current = row[column];
      if (current === null || current === undefined) { cell.textContent = 'NULL'; cell.className = 'null'; }
      else if (typeof current === 'object') cell.textContent = JSON.stringify(current);
      else cell.textContent = String(current);
      tr.append(cell);
    }
    body.append(tr);
  }
  table.append(head, body);
  grid.append(table);
  if (result.rows.length > visibleRows.length) {
    const note = document.createElement('div');
    note.className = 'query-hint result-note';
    note.textContent = `Showing ${visibleRows.length} of ${result.rows.length} rows in the viewport.`;
    grid.after(note);
  }
}

async function runQuery(script = false) {
  const id = byId('query-connection').value;
  const sql = script ? byId('sql-editor').value : selectedQueryText();
  const selectionActive = !script && byId('sql-editor').selectionStart !== byId('sql-editor').selectionEnd;
  if (!id) { byId('query-status').textContent = 'Select a connected connection'; return; }
  if (!sql.trim()) { byId('query-status').textContent = 'SQL is empty'; return; }
  saveCurrentQueryTab();
  state.currentQueryId = id;
  state.currentQueryOperationId = operationId();
  state.cancelRequested = false;
  byId('run-query').disabled = true;
  byId('run-script').disabled = true;
  byId('cancel-query').hidden = false;
  byId('query-status').textContent = script ? 'Running script…' : selectionActive ? 'Running selection…' : 'Running query…';
  byId('result-error').textContent = '';
  try {
    const result = await invoke(script ? 'execute_script' : 'execute_query', { id, sql, operationId: state.currentQueryOperationId });
    renderResult(result);
    byId('query-status').textContent = 'Completed';
  } catch (error) {
    renderResult(null);
    byId('result-error').textContent = state.cancelRequested ? 'Query cancelled.' : 'Query failed. Check the connection and SQL.';
    byId('query-status').textContent = state.cancelRequested ? 'Cancelled' : 'Error';
  } finally {
    state.currentQueryId = null;
    state.currentQueryOperationId = null;
    state.cancelRequested = false;
    byId('run-query').disabled = false;
    byId('run-script').disabled = false;
    byId('cancel-query').hidden = true;
  }
}

async function cancelQuery() {
  if (!state.currentQueryId) return;
  state.cancelRequested = true;
  byId('query-status').textContent = 'Cancelling…';
  try { await invoke('cancel_query', { id: state.currentQueryId, operationId: state.currentQueryOperationId }); } catch (error) { /* query result reports the final state */ }
}

function downloadResult(format) {
  if (!state.result) return;
  let content;
  let type;
  let extension;
  if (format === 'json') {
    content = JSON.stringify(state.result.rows, null, 2);
    type = 'application/json';
    extension = 'json';
  } else {
    const quote = (value) => { const text = value === null || value === undefined ? '' : String(value); return /[",\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text; };
    content = [state.result.columns, ...state.result.rows.map((row) => state.result.columns.map((column) => row[column]))].map((row) => row.map(quote).join(',')).join('\n');
    type = 'text/csv';
    extension = 'csv';
  }
  const url = URL.createObjectURL(new Blob([content], { type }));
  const link = document.createElement('a');
  link.href = url; link.download = `draco-result.${extension}`; link.click(); URL.revokeObjectURL(url);
}

async function loadHistory() {
  const list = byId('history-list');
  list.replaceChildren();
  const entries = await invoke('list_history');
  if (!entries.length) { const empty = document.createElement('div'); empty.className = 'empty-state'; empty.textContent = 'No query history'; list.append(empty); return; }
  for (const entry of entries) {
    const item = document.createElement('article'); item.className = 'history-item';
    const sql = document.createElement('code'); sql.textContent = entry.sql;
    const meta = document.createElement('small'); meta.textContent = `${entry.conn_label} · ${entry.row_count} rows · ${entry.duration_ms} ms`;
    const remove = document.createElement('button'); remove.className = 'button small danger history-meta'; remove.type = 'button'; remove.textContent = 'Delete';
    remove.addEventListener('click', async (event) => { event.stopPropagation(); await invoke('delete_history_entry', { id: entry.id }); loadHistory(); });
    item.append(sql, meta, remove); item.addEventListener('click', () => { byId('sql-editor').value = entry.sql; switchView('query'); }); list.append(item);
  }
}

async function loadSnippets() {
  const list = byId('snippet-list'); list.replaceChildren();
  const snippets = await invoke('list_snippets');
  if (!snippets.length) { const empty = document.createElement('div'); empty.className = 'empty-state'; empty.textContent = 'No saved snippets'; list.append(empty); return; }
  for (const snippet of snippets) {
    const item = document.createElement('article'); item.className = 'snippet-item';
    const name = document.createElement('strong'); name.textContent = snippet.name;
    const sql = document.createElement('code'); sql.textContent = snippet.sql;
    const meta = document.createElement('small'); meta.textContent = snippet.conn_label || 'All connections';
    const remove = document.createElement('button'); remove.className = 'button small danger snippet-meta'; remove.type = 'button'; remove.textContent = 'Delete';
    remove.addEventListener('click', async (event) => { event.stopPropagation(); await invoke('delete_snippet', { id: snippet.id }); loadSnippets(); });
    item.append(name, sql, meta, remove); item.addEventListener('click', () => { byId('sql-editor').value = snippet.sql; switchView('query'); }); list.append(item);
  }
}

function filterExplorerTree() {
  const query = state.explorerFilter.trim().toLowerCase();
  for (const group of byId('explorer-tree').querySelectorAll('.tree-group')) {
    const schemaButton = group.querySelector('.tree-item');
    const children = group.querySelector('.tree-children');
    const tableItems = children ? children.querySelectorAll('.tree-item') : [];
    const schemaMatches = schemaButton?.textContent.toLowerCase().includes(query);
    let tableMatches = false;
    for (const item of tableItems) {
      const matches = !query || item.textContent.toLowerCase().includes(query);
      item.hidden = !matches;
      tableMatches ||= matches;
    }
    group.hidden = Boolean(query) && !schemaMatches && !tableMatches;
  }
}

function formatEstimatedRows(value) {
  return `${new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 }).format(Math.max(0, Number(value) || 0))} rows`;
}

function renderExplorerTables(id, schemaName, children, tables) {
  children.replaceChildren();
  for (const table of tables) {
    const tableItem = document.createElement('div');
    tableItem.className = 'tree-item';
    const tableName = document.createElement('span');
    tableName.className = 'tree-item-label';
    tableName.textContent = `${table.kind === 'view' ? '◇' : '▦'} ${table.name}`;
    const estimate = document.createElement('small');
    estimate.textContent = formatEstimatedRows(table.estimated_rows);
    tableItem.append(tableName, estimate);
    tableItem.addEventListener('click', () => openTable(id, schemaName, table.name));
    children.append(tableItem);
  }
}

async function loadExplorerTablesForFilter(id) {
  const request = ++state.explorerFilterRequest;
  const groups = [...byId('explorer-tree').querySelectorAll('.tree-group')];
  await Promise.all(groups.filter((group) => group.dataset.loaded !== 'true').map(async (group) => {
    const schemaName = group.dataset.schema;
    if (!schemaName) return;
    try {
      const tables = await invoke('list_tables', { id, schema: schemaName });
      if (request !== state.explorerFilterRequest) return;
      renderExplorerTables(id, schemaName, group.querySelector('.tree-children'), tables);
      group.dataset.loaded = 'true';
    } catch {
      group.dataset.loaded = 'true';
    }
  }));
  if (request === state.explorerFilterRequest) filterExplorerTree();
}

async function openExplorer(id) {
  state.selectedConnectionId = id;
  state.selectedSchema = null;
  state.explorerFilter = '';
  byId('explorer-filter').value = '';
  byId('open-erd').disabled = true;
  const connection = state.connections.find((item) => item.id === id);
  if (!connection) return;
  if (connection.state !== 'connected') await connect(id);
  renderExplorerConnections();
  const current = state.connections.find((item) => item.id === id);
  if (current?.state !== 'connected') {
    byId('explorer-status').textContent = 'Connect failed';
    return;
  }
  byId('explorer-status').textContent = current.label;
  const tree = byId('explorer-tree');
  tree.replaceChildren();
  const loading = document.createElement('div');
  loading.className = 'empty-state';
  loading.textContent = 'Loading schemas…';
  tree.append(loading);
  try {
    const schemas = await invoke('list_schemas', { id });
    tree.replaceChildren();
    for (const schema of schemas) {
      const group = document.createElement('div');
      group.className = 'tree-group';
      group.dataset.schema = schema.name;
      group.dataset.loaded = 'false';
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'tree-item';
      button.textContent = `▸ ${schema.name}`;
      const children = document.createElement('div');
      children.className = 'tree-children';
      button.addEventListener('click', async () => {
        state.selectedSchema = schema.name;
        byId('open-erd').disabled = false;
        if (group.dataset.loaded === 'true') {
          children.hidden = !children.hidden;
          button.textContent = `${children.hidden ? '▸' : '▾'} ${schema.name}`;
          return;
        }
        button.textContent = `▾ ${schema.name}`;
        const tables = await invoke('list_tables', { id, schema: schema.name });
        renderExplorerTables(id, schema.name, children, tables);
        group.dataset.loaded = 'true';
        filterExplorerTree();
      });
      group.append(button, children);
      tree.append(group);
      filterExplorerTree();
    }
    if (!schemas.length) tree.innerHTML = '<div class="empty-state"><strong>No schemas</strong></div>';
  } catch (error) {
    tree.innerHTML = '<div class="empty-state"><strong>Could not load schemas</strong><span>Retry after reconnecting.</span></div>';
  }
}

function switchView(name) {
  for (const button of document.querySelectorAll('[data-view]')) {
    const active = button.dataset.view === name;
    button.classList.toggle('active', active);
    if (active) button.setAttribute('aria-current', 'page'); else button.removeAttribute('aria-current');
  }
  for (const view of ['connections', 'explorer', 'dashboard', 'admin', 'backup', 'assistant', 'query', 'history', 'snippets', 'table-detail', 'erd']) byId(`view-${view}`).hidden = view !== name;
  byId('page-title').textContent = name[0].toUpperCase() + name.slice(1);
  if (name === 'explorer') renderExplorerConnections();
  if (name === 'history') loadHistory();
  if (name === 'snippets') loadSnippets();
  if (name === 'dashboard') loadDashboard(byId('dashboard-connection').value);
  if (name === 'admin') loadAdmin(byId('admin-connection').value);
  if (name === 'assistant') loadAssistant(byId('assistant-connection').value);
}

const paletteCommands = [
  ['Connections', 'Manage saved PostgreSQL connections', 'connections'],
  ['Explorer', 'Browse schemas and tables', 'explorer'],
  ['Dashboard', 'Inspect database health and metrics', 'dashboard'],
  ['Administration', 'Review activity, locks and extensions', 'admin'],
  ['SQL Editor', 'Open a query workspace', 'query'],
  ['History', 'Reopen a recent query', 'history'],
  ['Snippets', 'Open reusable SQL snippets', 'snippets'],
];

let commandSearchRequest = 0;

async function renderCommandPalette(filter = '') {
  const list = byId('command-list');
  list.replaceChildren();
  const query = filter.trim().toLowerCase();
  const commands = paletteCommands.filter(([name, description]) => `${name} ${description}`.toLowerCase().includes(query));
  if (!commands.length && query.length < 2) { list.append(errorState('No commands found', 'Try another search term.')); return; }
  for (const [name, description, view] of commands) {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = 'command-item';
    const title = document.createElement('strong'); title.textContent = name;
    const hint = document.createElement('small'); hint.textContent = description;
    item.append(title, hint);
    item.addEventListener('click', () => { closeCommandPalette(); switchView(view); });
    list.append(item);
  }
  if (query.length < 2 || !state.selectedConnectionId) {
    if (!commands.length) list.append(errorState('Search the database', 'Connect to PostgreSQL and type at least two characters.'));
    return;
  }
  const request = ++commandSearchRequest;
  const loading = errorState('Searching database…', 'Looking through tables, views, columns and functions.');
  list.append(loading);
  try {
    const results = await invoke('global_search', { id: state.selectedConnectionId, term: query });
    if (request !== commandSearchRequest || byId('command-palette').hidden) return;
    loading.remove();
    if (!results.length) { list.append(errorState('No database objects found', 'Try another term or schema.')); return; }
    for (const result of results) {
      const item = document.createElement('button');
      item.type = 'button'; item.className = 'command-item';
      const title = document.createElement('strong');
      title.textContent = `${result.kind} · ${result.name}`;
      const hint = document.createElement('small');
      hint.textContent = `${result.schema}${result.table ? `.${result.table}` : ''}${result.detail ? ` · ${result.detail}` : ''}`;
      item.append(title, hint);
      item.addEventListener('click', () => {
        closeCommandPalette();
        if (result.table) openTable(state.selectedConnectionId, result.schema, result.table);
        else switchView('explorer');
      });
      list.append(item);
    }
  } catch (error) {
    if (request === commandSearchRequest) { loading.replaceChildren(); loading.append(errorState('Database search unavailable', 'Reconnect and try again.')); }
  }
}

function openCommandPalette() {
  const palette = byId('command-palette');
  palette.hidden = false;
  byId('command-search').value = '';
  renderCommandPalette();
  byId('command-search').focus();
}

function closeCommandPalette() { byId('command-palette').hidden = true; }

function finishDialog(result) {
  const resolver = dialogResolver;
  dialogResolver = null;
  byId('app-dialog').hidden = true;
  byId('app-dialog-input').value = '';
  resolver?.(result);
}

function showDialog({ title, message, eyebrow = 'DRACO', kind = 'confirm', confirmLabel = 'Confirm', cancelLabel = 'Cancel', inputLabel = 'Value', placeholder = '' }) {
  if (dialogResolver) finishDialog(null);
  const dialog = byId('app-dialog');
  const inputWrap = byId('app-dialog-input-wrap');
  const cancel = byId('app-dialog-cancel');
  const confirm = byId('app-dialog-confirm');
  byId('app-dialog-eyebrow').textContent = eyebrow;
  byId('app-dialog-title').textContent = title;
  byId('app-dialog-message').textContent = message;
  cancel.textContent = cancelLabel;
  confirm.textContent = confirmLabel;
  confirm.classList.toggle('danger-action', kind === 'confirm-danger');
  inputWrap.hidden = kind !== 'prompt';
  byId('app-dialog-input-label').textContent = inputLabel;
  byId('app-dialog-input').placeholder = placeholder;
  cancel.hidden = kind === 'alert';
  dialog.hidden = false;
  const result = new Promise((resolve) => { dialogResolver = resolve; });
  const focusTarget = kind === 'prompt' ? byId('app-dialog-input') : confirm;
  window.setTimeout(() => focusTarget.focus(), 0);
  return result;
}

function showAlert(message, title = 'Notice') {
  return showDialog({ title, message, kind: 'alert', confirmLabel: 'OK' });
}

function showConfirm(message, title = 'Confirm action', danger = false) {
  return showDialog({ title, message, kind: danger ? 'confirm-danger' : 'confirm', confirmLabel: danger ? 'Delete' : 'Confirm' });
}

function showPrompt(message, title = 'Enter a value', inputLabel = 'Value', placeholder = '') {
  return showDialog({ title, message, kind: 'prompt', confirmLabel: 'Save', inputLabel, placeholder }).then((accepted) => accepted === null ? null : byId('app-dialog-input').value.trim());
}

function bindAppDialog() {
  byId('app-dialog-confirm').addEventListener('click', () => {
    const input = byId('app-dialog-input');
    finishDialog(byId('app-dialog-input-wrap').hidden ? true : input.value.trim());
  });
  byId('app-dialog-cancel').addEventListener('click', () => finishDialog(false));
  byId('app-dialog').querySelector('[data-close-app-dialog]').addEventListener('click', () => finishDialog(false));
  byId('app-dialog-input').addEventListener('keydown', (event) => {
    if (event.key === 'Enter') { event.preventDefault(); byId('app-dialog-confirm').click(); }
  });
}

function setMaximizeControl(maximized) {
  const button = byId('window-maximize');
  if (!button) return;
  button.textContent = maximized ? '❐' : '□';
  button.title = maximized ? 'Restore' : 'Maximize';
  button.setAttribute('aria-label', maximized ? 'Restore' : 'Maximize');
}

async function syncWindowState() {
  if (!currentWindow) return;
  try { setMaximizeControl(await currentWindow.isMaximized()); } catch { /* non-critical UI state */ }
}

function bindWindowControls() {
  const topbar = document.querySelector('.topbar');
  const minimize = byId('window-minimize');
  const maximize = byId('window-maximize');
  const close = byId('window-close');
  if (!topbar || !minimize || !maximize || !close) return;

  topbar.addEventListener('mousedown', (event) => {
    if (event.button !== 0 || !currentWindow) return;
    const target = event.target instanceof HTMLElement ? event.target : null;
    if (target?.closest('button, input, select, textarea, a, [role="button"], .window-controls')) return;
    void currentWindow.startDragging();
  });
  minimize.addEventListener('click', () => { void currentWindow?.minimize(); });
  maximize.addEventListener('click', async () => {
    if (!currentWindow) return;
    try { await currentWindow.toggleMaximize(); await syncWindowState(); } catch { /* keep the shell usable */ }
  });
  close.addEventListener('click', () => { void currentWindow?.close(); });
  setMaximizeControl(false);
  void syncWindowState();
}

function bindSidebarToggle() {
  const sidebar = document.querySelector('.sidebar');
  const toggle = byId('sidebar-toggle');
  if (!sidebar || !toggle) return;
  toggle.addEventListener('click', () => {
    const collapsed = sidebar.classList.toggle('collapsed');
    toggle.setAttribute('aria-expanded', String(!collapsed));
    toggle.setAttribute('aria-label', collapsed ? 'Show sidebar' : 'Hide sidebar');
    toggle.title = collapsed ? 'Show sidebar' : 'Hide sidebar';
  });
}

async function boot() {
  try {
    if (!invoke) throw new Error('Tauri IPC is unavailable');
    const health = await invoke('health');
    byId('health-label').textContent = health.ready ? 'Backend ready' : 'Backend unavailable';
    byId('health-value').textContent = health.ready ? 'Bridge online' : 'Bridge offline';
    await refreshConnections();
  } catch (error) {
    byId('health-label').textContent = 'Backend error';
    byId('health-value').textContent = 'Bridge unavailable';
  }
}

byId('new-connection').addEventListener('click', () => showForm());
byId('open-command-palette').addEventListener('click', openCommandPalette);
for (const element of document.querySelectorAll('[data-close-command-palette]')) element.addEventListener('click', closeCommandPalette);
byId('command-search').addEventListener('input', (event) => renderCommandPalette(event.target.value));
document.addEventListener('keydown', (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') { event.preventDefault(); openCommandPalette(); }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 't') { event.preventDefault(); switchView('query'); newQueryTab(); }
  if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === 's') { event.preventDefault(); switchView('query'); void saveCurrentSnippet(); }
  if (event.key === 'F8') { event.preventDefault(); switchView('query'); runQuery(false); }
  if (event.key === 'F10') { event.preventDefault(); switchView('query'); runQuery(true); }
  if (event.key === 'Escape' && !byId('command-palette').hidden) closeCommandPalette();
});
byId('cancel-connection').addEventListener('click', hideForm);
byId('connection-form').addEventListener('submit', saveCurrentConnection);
byId('test-connection').addEventListener('click', testCurrentConnection);
byId('format-sql').addEventListener('click', () => {
  const editor = byId('sql-editor');
  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  const source = start !== end ? editor.value.slice(start, end) : editor.value;
  const formatted = formatSql(source);
  if (start !== end) editor.setRangeText(formatted, start, end, 'select');
  else editor.value = formatted;
  saveCurrentQueryTab();
  byId('query-status').textContent = 'SQL formatted';
});
byId('run-query').addEventListener('click', () => runQuery(false));
byId('run-script').addEventListener('click', () => runQuery(true));
byId('cancel-query').addEventListener('click', cancelQuery);
byId('export-csv').addEventListener('click', () => downloadResult('csv'));
byId('export-json').addEventListener('click', () => downloadResult('json'));
byId('save-current-snippet').addEventListener('click', saveCurrentSnippet);
byId('clear-history').addEventListener('click', async () => { if (await showConfirm('Clear all saved query history?', 'Clear history', true)) { await invoke('clear_history'); loadHistory(); } });
byId('sql-editor').addEventListener('keydown', (event) => { if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') { event.preventDefault(); runQuery(false); } });
byId('open-erd').addEventListener('click', openErd);
byId('explorer-filter').addEventListener('input', (event) => {
  state.explorerFilter = event.target.value;
  filterExplorerTree();
  if (state.explorerFilter.trim() && state.selectedConnectionId) void loadExplorerTablesForFilter(state.selectedConnectionId);
});
byId('dashboard-connection').addEventListener('change', () => loadDashboard(byId('dashboard-connection').value));
byId('admin-connection').addEventListener('change', () => loadAdmin(byId('admin-connection').value));
byId('backup-connection').addEventListener('change', () => { const connection = state.connections.find((item) => item.id === byId('backup-connection').value); if (connection) byId('restore-database').value = connection.database; });
byId('assistant-connection').addEventListener('change', () => loadAssistant(byId('assistant-connection').value));
byId('run-backup').addEventListener('click', () => runBackup(false));
byId('run-restore').addEventListener('click', () => runBackup(true));
byId('cancel-operation').addEventListener('click', cancelOperation);
byId('send-assistant').addEventListener('click', sendAssistant);
byId('clear-assistant').addEventListener('click', async () => { const id = byId('assistant-connection').value; if (!id) return; await invoke('clear_assistant_history', { id }); loadAssistant(id); });
for (const button of document.querySelectorAll('[data-view]')) button.addEventListener('click', () => switchView(button.dataset.view));
bindWindowControls();
bindSidebarToggle();
bindAppDialog();
renderQueryTabs();
boot();
