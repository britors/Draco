import { resultRowToText, resultToTsv, serializeResult } from './result-export.js';
import { highlightSql } from './sql-highlight.js';
import { applySuggestion, buildCompletionIndex, suggest } from './sql-autocomplete.js';
import { visibleRange } from './virtual-list.js';

const invoke = window.__TAURI__?.core?.invoke;
const currentWindow = window.__TAURI__?.window?.getCurrentWindow?.();
const state = { connections: [], selectedConnectionId: null, selectedSchema: null, explorerFilter: '', explorerFilterRequest: 0, lastTested: null, result: null, currentQueryId: null, currentQueryOperationId: null, cancelRequested: false, currentOperationId: null, preferences: { version: '2.0.3', theme: 'dark', accent: 'coral', check_updates_on_startup: true }, releaseUrl: '', queryTabs: [{ id: 1, label: 'Query 1', sql: '' }], currentQueryTabId: 1 };
let dialogResolver = null;

const PIX_KEY = 'britors@live.com';
const PIX_COPY_AND_PASTE = '00020126380014BR.GOV.BCB.PIX0116britors@live.com5204000053039865802BR5906BRITOR6009SAO PAULO62070503***63044B68';
const ACCENTS = {
  coral: ['#d96558', '#ffb09a', '217, 101, 88'],
  blue: ['#507bd8', '#a9c4ff', '80, 123, 216'],
  green: ['#3f9f72', '#98e0b7', '63, 159, 114'],
  purple: ['#8a62d1', '#cdb4ff', '138, 98, 209'],
  amber: ['#c48932', '#f2c879', '196, 137, 50'],
};

const byId = (id) => document.getElementById(id);
const value = (id) => byId(id).value.trim();
const optional = (id) => value(id) || null;
const numberOrNull = (id) => value(id) ? Number(value(id)) : null;

function applyAppearance(preferences) {
  const root = document.documentElement;
  const accent = ACCENTS[preferences.accent] || ACCENTS.coral;
  root.dataset.theme = preferences.theme === 'light' ? 'light' : 'dark';
  root.dataset.accent = ACCENTS[preferences.accent] ? preferences.accent : 'coral';
  root.style.setProperty('--color-action', accent[0]);
  root.style.setProperty('--color-action-soft', accent[1]);
  root.style.setProperty('--focus-ring', `0 0 0 3px rgba(${accent[2]}, .22)`);
  document.querySelector('meta[name="color-scheme"]')?.setAttribute('content', root.dataset.theme);
}

function syncPreferenceControls() {
  applyAppearance(state.preferences);
  for (const button of document.querySelectorAll('[data-theme-choice]')) button.setAttribute('aria-pressed', String(button.dataset.themeChoice === state.preferences.theme));
  for (const button of document.querySelectorAll('[data-accent-choice]')) button.setAttribute('aria-pressed', String(button.dataset.accentChoice === state.preferences.accent));
  byId('check-updates-startup').checked = state.preferences.check_updates_on_startup;
  byId('about-version').textContent = state.preferences.version;
  byId('update-detail').textContent = `Current version: ${state.preferences.version}`;
}

async function loadPreferences() {
  try {
    state.preferences = await invoke('preferences');
    syncPreferenceControls();
    return state.preferences;
  } catch {
    applyAppearance(state.preferences);
    byId('preferences-status').textContent = 'Could not load preferences';
    return state.preferences;
  }
}

async function savePreferences(patch) {
  state.preferences = { ...state.preferences, ...patch };
  syncPreferenceControls();
  byId('preferences-status').textContent = 'Saving…';
  try {
    state.preferences = await invoke('save_preferences', { preferences: state.preferences });
    syncPreferenceControls();
    byId('preferences-status').textContent = 'Saved locally';
  } catch {
    byId('preferences-status').textContent = 'Could not save';
  }
}

async function checkForUpdates(manual = true) {
  const button = byId('check-updates');
  button.disabled = true;
  byId('update-title').textContent = 'Checking for updates…';
  try {
    const update = await invoke('check_for_updates');
    state.releaseUrl = update.release_url;
    byId('copy-release-link').hidden = !update.update_available;
    byId('update-title').textContent = update.update_available ? `Draco ${update.latest_version} is available` : 'Draco is up to date';
    byId('update-detail').textContent = update.update_available ? `Installed: ${update.current_version} · Latest: ${update.latest_version}` : `Current version: ${update.current_version}`;
  } catch {
    byId('update-title').textContent = manual ? 'Could not check for updates' : 'Automatic update check unavailable';
    byId('update-detail').textContent = 'Check your network connection and try again.';
  } finally {
    button.disabled = false;
  }
}

function showPreferenceSection(name) {
  for (const tab of document.querySelectorAll('[data-preference-section]')) {
    const active = tab.dataset.preferenceSection === name;
    tab.classList.toggle('active', active);
    tab.setAttribute('aria-selected', String(active));
  }
  for (const panel of document.querySelectorAll('[data-preference-panel]')) panel.hidden = panel.dataset.preferencePanel !== name;
}

function syncEditorHighlight() {
  const editor = byId('sql-editor');
  const overlay = byId('sql-editor-highlight');
  overlay.innerHTML = highlightSql(editor.value);
  overlay.scrollTop = editor.scrollTop;
  overlay.scrollLeft = editor.scrollLeft;
}

function setEditorValue(text) {
  byId('sql-editor').value = text;
  syncEditorHighlight();
  hideAutocomplete();
}

const completionCache = new Map();
let autocompleteItems = [];
let autocompleteActive = 0;

async function completionIndexFor(id) {
  if (!id) return buildCompletionIndex(null);
  if (completionCache.has(id)) return completionCache.get(id);
  try {
    const data = await invoke('completion_data', { id });
    const index = buildCompletionIndex(data);
    completionCache.set(id, index);
    return index;
  } catch {
    return buildCompletionIndex(null);
  }
}

function hideAutocomplete() {
  const popup = byId('sql-autocomplete');
  popup.hidden = true;
  popup.replaceChildren();
  autocompleteItems = [];
}

function setActiveSuggestion(index) {
  const popup = byId('sql-autocomplete');
  const options = [...popup.querySelectorAll('.sql-suggestion')];
  options.forEach((option, i) => {
    option.classList.toggle('active', i === index);
    option.setAttribute('aria-selected', String(i === index));
  });
  autocompleteActive = index;
  options[index]?.scrollIntoView({ block: 'nearest' });
}

function acceptSuggestion(index = autocompleteActive) {
  const suggestion = autocompleteItems[index];
  if (!suggestion) return;
  const editor = byId('sql-editor');
  const { text, caret } = applySuggestion(editor.value, editor.selectionStart, suggestion);
  editor.value = text;
  editor.setSelectionRange(caret, caret);
  syncEditorHighlight();
  saveCurrentQueryTab();
  hideAutocomplete();
  editor.focus();
}

// A hidden mirror of the textarea (same font/padding/border/wrapping) lets us measure where a
// caret offset lands in pixels — textareas have no native API for this.
function caretPixelPosition(textarea, offset) {
  const mirror = document.createElement('div');
  const style = window.getComputedStyle(textarea);
  for (const prop of ['boxSizing', 'width', 'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft', 'borderTopWidth', 'borderRightWidth', 'borderBottomWidth', 'borderLeftWidth', 'fontFamily', 'fontSize', 'fontWeight', 'lineHeight', 'letterSpacing', 'tabSize']) {
    mirror.style[prop] = style[prop];
  }
  mirror.style.position = 'absolute';
  mirror.style.visibility = 'hidden';
  mirror.style.whiteSpace = 'pre-wrap';
  mirror.style.wordWrap = 'break-word';
  mirror.style.top = '0';
  mirror.style.left = '-9999px';
  mirror.style.height = 'auto';
  document.body.append(mirror);
  mirror.append(document.createTextNode(textarea.value.slice(0, offset)));
  const marker = document.createElement('span');
  marker.textContent = '​';
  mirror.append(marker);
  const lineHeight = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.2;
  const { offsetLeft, offsetTop } = marker;
  mirror.remove();
  return { top: offsetTop + lineHeight - textarea.scrollTop, left: offsetLeft - textarea.scrollLeft };
}

function renderAutocomplete(items, position) {
  autocompleteItems = items;
  autocompleteActive = 0;
  const popup = byId('sql-autocomplete');
  popup.replaceChildren();
  items.forEach((item, index) => {
    const option = document.createElement('li');
    option.role = 'option';
    option.id = `sql-suggestion-${index}`;
    option.className = `sql-suggestion${index === 0 ? ' active' : ''}`;
    option.setAttribute('aria-selected', String(index === 0));
    const label = document.createElement('span');
    label.className = 'sql-suggestion-label';
    label.textContent = item.label;
    const detail = document.createElement('span');
    detail.className = 'sql-suggestion-detail';
    detail.textContent = item.detail;
    option.append(label, detail);
    option.addEventListener('mousedown', (event) => { event.preventDefault(); acceptSuggestion(index); });
    popup.append(option);
  });
  popup.hidden = false;
  popup.style.top = `${position.top}px`;
  popup.style.left = `${position.left}px`;
}

async function updateAutocomplete() {
  const editor = byId('sql-editor');
  if (editor.selectionStart !== editor.selectionEnd) { hideAutocomplete(); return; }
  const index = await completionIndexFor(byId('query-connection').value);
  const suggestions = suggest(index, editor.value, editor.selectionStart);
  if (!suggestions.length) { hideAutocomplete(); return; }
  renderAutocomplete(suggestions, caretPixelPosition(editor, editor.selectionStart));
}

function saveCurrentQueryTab() {
  const tab = state.queryTabs.find((item) => item.id === state.currentQueryTabId);
  if (tab) tab.sql = byId('sql-editor').value;
}

function renderQueryTabs() {
  const tabs = byId('query-tabs');
  if (!tabs) return;
  tabs.replaceChildren();
  for (const tab of state.queryTabs) {
    const wrap = document.createElement('div');
    wrap.className = 'query-tab-wrap';
    const selected = tab.id === state.currentQueryTabId;
    wrap.classList.toggle('active', selected);
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'query-tab';
    button.role = 'tab';
    button.ariaSelected = String(selected);
    button.textContent = tab.label;
    button.title = 'Double-click to rename';
    button.addEventListener('click', () => selectQueryTab(tab.id));
    button.addEventListener('dblclick', (event) => { event.preventDefault(); void renameQueryTab(tab.id); });
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'query-tab-close';
    close.textContent = '×';
    close.title = `Close ${tab.label}`;
    close.setAttribute('aria-label', `Close ${tab.label}`);
    close.addEventListener('click', (event) => { event.stopPropagation(); closeQueryTab(tab.id); });
    wrap.append(button, close);
    tabs.append(wrap);
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
  setEditorValue(tab.sql);
  renderQueryTabs();
  byId('sql-editor').focus();
}

function newQueryTab() {
  saveCurrentQueryTab();
  const next = Math.max(...state.queryTabs.map((item) => item.id), 0) + 1;
  state.queryTabs.push({ id: next, label: `Query ${next}`, sql: '' });
  state.currentQueryTabId = next;
  setEditorValue('');
  renderQueryTabs();
  byId('sql-editor').focus();
}

function closeQueryTab(id) {
  const index = state.queryTabs.findIndex((item) => item.id === id);
  if (index === -1) return;
  if (state.queryTabs.length === 1) {
    state.queryTabs = [{ id, label: 'Query 1', sql: '' }];
    state.currentQueryTabId = id;
    setEditorValue('');
    renderQueryTabs();
    return;
  }
  state.queryTabs.splice(index, 1);
  if (state.currentQueryTabId === id) {
    const next = state.queryTabs[Math.max(0, index - 1)];
    state.currentQueryTabId = next.id;
    setEditorValue(next.sql);
  }
  renderQueryTabs();
}

async function renameQueryTab(id) {
  const tab = state.queryTabs.find((item) => item.id === id);
  if (!tab) return;
  const name = await showPrompt('Choose a name for this query tab.', 'Rename tab', 'Tab name', '', tab.label);
  if (!name) return;
  tab.label = name;
  renderQueryTabs();
}

function openSqlInNewTab(sql, connectionId) {
  switchView('query');
  newQueryTab();
  const tab = state.queryTabs.find((item) => item.id === state.currentQueryTabId);
  const firstLine = sql.split(/\r?\n/, 1)[0].trim();
  if (tab) {
    tab.sql = sql;
    tab.label = firstLine.length > 28 ? `${firstLine.slice(0, 28)}…` : firstLine || tab.label;
  }
  setEditorValue(sql);
  byId('query-connection').value = connectionId;
  renderQueryTabs();
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
    renderQueryConnections();
    renderAdvancedConnections();
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

let assistantRequestEpoch = 0;

async function loadAssistant(id) {
  const requestEpoch = ++assistantRequestEpoch;
  byId('send-assistant').disabled = false;
  byId('assistant-status').textContent = '';
  if (!id) { renderAssistantHistory([]); return; }
  try {
    const history = await invoke('assistant_history', { id });
    if (requestEpoch === assistantRequestEpoch) renderAssistantHistory(history);
  } catch (error) {
    if (requestEpoch === assistantRequestEpoch) { renderAssistantHistory([]); byId('assistant-status').textContent = 'Could not load assistant history.'; }
  }
}

async function sendAssistant() {
  const id = byId('assistant-connection').value; const message = byId('assistant-message').value;
  if (!id) { byId('assistant-status').textContent = 'Select a connected connection'; return; }
  if (!message.trim()) return;
  const requestEpoch = ++assistantRequestEpoch;
  byId('send-assistant').disabled = true; byId('assistant-status').textContent = 'Thinking…';
  try {
    const reply = await invoke('assistant_send', { id, message });
    if (requestEpoch === assistantRequestEpoch) { renderAssistantHistory(reply.history); byId('assistant-message').value = ''; byId('assistant-status').textContent = `${reply.input_tokens} input · ${reply.output_tokens} output tokens`; }
  } catch (error) { if (requestEpoch === assistantRequestEpoch) byId('assistant-status').textContent = 'Assistant request failed. Configure a provider key and reconnect.'; }
  finally { if (requestEpoch === assistantRequestEpoch) byId('send-assistant').disabled = false; }
}

async function askAssistantAboutQueryStat(id, queryStat) {
  byId('assistant-connection').value = id;
  byId('assistant-message').value = `Analyze this query for performance. pg_stat_statements recorded ${queryStat.calls} calls, ${queryStat.mean_exec_ms.toFixed(1)} ms mean execution time, ${queryStat.total_exec_ms.toFixed(1)} ms total execution time, and ${queryStat.rows} rows returned in total.\n\n\`\`\`sql\n${queryStat.query}\n\`\`\``;
  switchView('assistant');
  await sendAssistant();
}

function operationId() { return `ui-${Date.now()}-${Math.random().toString(36).slice(2)}`; }

async function runBackup(restore = false) {
  const id = byId('backup-connection').value; if (!id) { byId('backup-status').textContent = 'Select a connected connection'; return; }
  if (restore && !await showConfirm('Restore will write data into the selected database. Continue only if the input and target are correct.', 'Confirm restore', true, 'Restore')) return;
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

function codePanel(title, text) {
  const panel = document.createElement('section'); panel.className = 'data-panel code-panel';
  const heading = document.createElement('h3'); heading.textContent = title;
  const code = document.createElement('pre'); code.className = 'ddl-code'; code.textContent = text || '—';
  panel.append(heading, code);
  return panel;
}

function tableMaintenancePanel(id, schema, table) {
  const panel = document.createElement('section'); panel.className = 'data-panel maintenance-panel';
  const heading = document.createElement('div'); heading.className = 'maintenance-heading';
  const title = document.createElement('h3'); title.textContent = 'Table maintenance';
  const actions = document.createElement('div'); actions.className = 'maintenance-actions';
  const status = document.createElement('div'); status.className = 'form-status maintenance-status'; status.setAttribute('role', 'status');
  const operations = [['vacuum', 'Vacuum'], ['analyze', 'Analyze'], ['vacuum_analyze', 'Vacuum + Analyze'], ['vacuum_full', 'Vacuum Full']];
  for (const [operation, label] of operations) {
    const button = document.createElement('button'); button.className = `button small ${operation === 'vacuum_full' ? 'danger' : ''}`; button.type = 'button'; button.textContent = label;
    button.addEventListener('click', async () => {
      if (operation === 'vacuum_full') {
        const objectName = `${schema}.${table}`;
        const confirmation = await showDangerPrompt(`VACUUM FULL rewrites ${objectName} and holds an ACCESS EXCLUSIVE lock. Type ${objectName} to continue.`, 'Run VACUUM FULL', 'Schema and table', objectName);
        if (confirmation !== objectName) { if (confirmation !== null) await showAlert('The table name did not match. Maintenance was not started.', 'VACUUM FULL cancelled'); return; }
      }
      for (const control of actions.querySelectorAll('button')) control.disabled = true;
      status.textContent = `Running ${label}…`; status.className = 'form-status maintenance-status';
      try {
        await invoke('run_table_maintenance', { id, schema, table, operation });
        status.textContent = `${label} completed`;
        status.className = 'form-status maintenance-status success';
      } catch (error) {
        status.textContent = error?.message || `${label} failed`;
        status.className = 'form-status maintenance-status error';
      } finally {
        for (const control of actions.querySelectorAll('button')) control.disabled = false;
      }
    });
    actions.append(button);
  }
  heading.append(title, actions); panel.append(heading, status);
  return panel;
}

function errorState(title, message) { const empty = document.createElement('div'); empty.className = 'empty-state'; const strong = document.createElement('strong'); strong.textContent = title; const span = document.createElement('span'); span.textContent = message; empty.append(strong, span); return empty; }

function unavailablePanel(title, message) {
  const panel = document.createElement('section'); panel.className = 'data-panel';
  panel.append(errorState(title, message));
  return panel;
}

function roleAttributes(role) {
  const attributes = [role.login ? 'LOGIN' : 'NOLOGIN'];
  if (role.superuser) attributes.push('SUPERUSER');
  if (role.create_database) attributes.push('CREATEDB');
  if (role.create_role) attributes.push('CREATEROLE');
  attributes.push(role.connection_limit === -1 ? 'unlimited connections' : `${role.connection_limit} connections`);
  if (role.valid_until) attributes.push(`valid until ${role.valid_until}`);
  return attributes.join(' · ');
}

function roleCheckbox(id, label) {
  const wrapper = document.createElement('label'); wrapper.className = 'check-row role-check';
  const input = document.createElement('input'); input.id = id; input.type = 'checkbox';
  wrapper.append(input, document.createTextNode(label));
  return wrapper;
}

function renderRolesPanel(id, roles) {
  const panel = document.createElement('section'); panel.className = 'data-panel roles-panel';
  const heading = document.createElement('div'); heading.className = 'roles-heading';
  const title = document.createElement('h3'); title.textContent = 'Roles';
  const count = document.createElement('span'); count.className = 'badge'; count.textContent = `${roles.length} roles`;
  heading.append(title, count);

  const form = document.createElement('form'); form.className = 'role-form';
  const nameLabel = document.createElement('label'); nameLabel.textContent = 'Role name';
  const name = document.createElement('input'); name.id = 'admin-role-name'; name.required = true; name.maxLength = 63; name.autocomplete = 'off'; name.placeholder = 'reporting_reader'; nameLabel.append(name);
  const limitLabel = document.createElement('label'); limitLabel.textContent = 'Connection limit';
  const limit = document.createElement('input'); limit.id = 'admin-role-limit'; limit.type = 'number'; limit.min = '-1'; limit.value = '-1'; limitLabel.append(limit);
  const options = document.createElement('div'); options.className = 'role-options';
  options.append(roleCheckbox('admin-role-login', 'Can login'), roleCheckbox('admin-role-createdb', 'Create databases'), roleCheckbox('admin-role-createrole', 'Create roles'), roleCheckbox('admin-role-superuser', 'Superuser'));
  const create = document.createElement('button'); create.className = 'button primary'; create.type = 'submit'; create.textContent = 'Create role';
  const status = document.createElement('div'); status.className = 'form-status role-status'; status.setAttribute('role', 'status');
  form.append(nameLabel, limitLabel, options, create, status);
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    create.disabled = true; status.textContent = 'Creating role…'; status.className = 'form-status role-status';
    try {
      await invoke('create_role', { id, input: { name: name.value.trim(), login: byId('admin-role-login').checked, create_database: byId('admin-role-createdb').checked, create_role: byId('admin-role-createrole').checked, superuser: byId('admin-role-superuser').checked, connection_limit: Number(limit.value) } });
      await loadAdmin(id);
    } catch (error) {
      create.disabled = false; status.textContent = error?.message || 'Could not create role'; status.className = 'form-status role-status error';
    }
  });

  const list = document.createElement('div'); list.className = 'role-list';
  for (const role of roles) {
    const row = document.createElement('div'); row.className = 'role-item';
    const detail = document.createElement('div');
    const roleName = document.createElement('strong'); roleName.textContent = role.name;
    const attributes = document.createElement('small'); attributes.textContent = roleAttributes(role);
    detail.append(roleName, attributes);
    const remove = document.createElement('button'); remove.className = 'button small danger'; remove.type = 'button'; remove.textContent = 'Delete';
    const reserved = role.name.toLowerCase().startsWith('pg_');
    remove.disabled = reserved; remove.title = reserved ? 'PostgreSQL system roles cannot be deleted here' : `Delete ${role.name}`;
    remove.addEventListener('click', async () => {
      const confirmation = await showDangerPrompt(`Type ${role.name} to permanently delete this PostgreSQL role. Roles that own objects cannot be deleted.`, 'Delete role', 'Role name', role.name);
      if (confirmation !== role.name) { if (confirmation !== null) await showAlert('The role name did not match. Nothing was deleted.', 'Role not deleted'); return; }
      try { await invoke('delete_role', { id, name: role.name }); await loadAdmin(id); }
      catch (error) { await showAlert(error?.message || 'Could not delete role', 'Role not deleted'); }
    });
    row.append(detail, remove); list.append(row);
  }
  panel.append(heading, form, list);
  return panel;
}

function renderActivityPanel(id, rows) {
  const panel = document.createElement('section'); panel.className = 'data-panel activity-panel';
  const heading = document.createElement('h3'); heading.textContent = 'Activity'; panel.append(heading);
  if (!rows.length) { panel.append(errorState('No other sessions', 'PostgreSQL reported no activity for this database.')); return panel; }
  for (const activity of rows.slice(0, 20)) {
    const row = document.createElement('div'); row.className = 'activity-item';
    const detail = document.createElement('div');
    const title = document.createElement('strong'); title.textContent = `PID ${activity.pid} · ${activity.usename || 'unknown user'} · ${activity.state || 'unknown'}`;
    const query = document.createElement('small'); query.textContent = activity.query || 'No current query'; query.title = activity.query || '';
    detail.append(title, query);
    const cancel = document.createElement('button'); cancel.className = 'button small danger'; cancel.type = 'button'; cancel.textContent = 'Cancel query';
    const active = activity.state === 'active'; cancel.disabled = !active; cancel.title = active ? `Cancel query on PID ${activity.pid}` : 'Only active queries can be cancelled';
    cancel.addEventListener('click', async () => {
      if (!await showConfirm(`Cancel the active query on PID ${activity.pid}? The PostgreSQL session will remain connected.`, 'Cancel active query', true, 'Cancel query')) return;
      try { await invoke('cancel_activity', { id, pid: activity.pid }); await loadAdmin(id); }
      catch (error) { await showAlert(error?.message || 'Could not cancel the query', 'Query not cancelled'); }
    });
    row.append(detail, cancel); panel.append(row);
  }
  return panel;
}

function renderCronJobsPanel(id, cron) {
  const panel = document.createElement('section'); panel.className = 'data-panel cron-panel';
  const heading = document.createElement('div'); heading.className = 'roles-heading';
  const title = document.createElement('h3'); title.textContent = 'Scheduled jobs · pg_cron';
  const status = document.createElement('span'); status.className = 'badge'; status.textContent = cron.installed ? `${cron.jobs.length} jobs` : 'Not installed';
  heading.append(title, status); panel.append(heading);
  if (!cron.installed) { panel.append(errorState('pg_cron is not installed', 'Install and preload pg_cron in PostgreSQL to manage scheduled jobs.')); return panel; }
  if (!cron.jobs.length) { panel.append(errorState('No scheduled jobs', 'pg_cron is available, but this database has no jobs.')); return panel; }
  for (const job of cron.jobs) {
    const row = document.createElement('div'); row.className = 'cron-item';
    const detail = document.createElement('div');
    const name = document.createElement('strong'); name.textContent = job.name || `Job ${job.id}`;
    const metadata = document.createElement('small'); metadata.textContent = `${job.schedule} · ${job.active ? 'active' : 'paused'} · last: ${job.last_status || 'never run'}${job.last_run ? ` at ${job.last_run} UTC` : ''}`;
    const command = document.createElement('code'); command.textContent = job.command; command.title = job.command;
    detail.append(name, metadata, command);
    const actions = document.createElement('div'); actions.className = 'cron-actions';
    const toggle = document.createElement('button'); toggle.className = 'button small'; toggle.type = 'button'; toggle.textContent = job.active ? 'Pause' : 'Resume';
    toggle.addEventListener('click', async () => {
      toggle.disabled = true;
      try { await invoke('set_cron_job_active', { id, jobId: job.id, active: !job.active }); await loadAdmin(id); }
      catch (error) { toggle.disabled = false; await showAlert(error?.message || 'Could not change the job state', 'Job not changed'); }
    });
    const remove = document.createElement('button'); remove.className = 'button small danger'; remove.type = 'button'; remove.textContent = 'Delete';
    remove.addEventListener('click', async () => {
      if (!await showConfirm(`Permanently delete the scheduled job “${job.name || job.id}”?`, 'Delete scheduled job', true)) return;
      try { await invoke('delete_cron_job', { id, jobId: job.id }); await loadAdmin(id); }
      catch (error) { await showAlert(error?.message || 'Could not delete the scheduled job', 'Job not deleted'); }
    });
    actions.append(toggle, remove); row.append(detail, actions); panel.append(row);
  }
  return panel;
}

function renderExtensionsPanel(id, extensions) {
  const panel = document.createElement('section'); panel.className = 'data-panel extensions-panel';
  const heading = document.createElement('div'); heading.className = 'roles-heading';
  const title = document.createElement('h3'); title.textContent = 'Extensions';
  const count = document.createElement('span'); count.className = 'badge'; count.textContent = `${extensions.installed.length} installed`;
  heading.append(title, count); panel.append(heading);
  const installedNames = new Set(extensions.installed.map((extension) => extension.name));
  const entries = [
    ...extensions.installed.map((extension) => ({ ...extension, installed: true })),
    ...extensions.available.filter((extension) => !installedNames.has(extension.name)).slice(0, 30).map((extension) => ({ ...extension, installed: false })),
  ];
  for (const extension of entries) {
    const row = document.createElement('div'); row.className = 'extension-item';
    const detail = document.createElement('div');
    const name = document.createElement('strong'); name.textContent = extension.name;
    const metadata = document.createElement('small'); metadata.textContent = extension.installed ? `installed ${extension.installed_version || ''}` : `available ${extension.default_version || ''}`; metadata.title = extension.comment || '';
    detail.append(name, metadata);
    const action = document.createElement('button'); action.className = `button small ${extension.installed ? 'danger' : ''}`; action.type = 'button'; action.textContent = extension.installed ? 'Drop' : 'Install';
    const protectedExtension = extension.name.toLowerCase() === 'plpgsql';
    action.disabled = protectedExtension; action.title = protectedExtension ? 'The built-in plpgsql extension is protected' : `${action.textContent} ${extension.name}`;
    action.addEventListener('click', async () => {
      if (extension.installed) {
        const confirmation = await showDangerPrompt(`Type ${extension.name} to drop this extension. PostgreSQL will refuse the operation when dependent objects exist.`, 'Drop extension', 'Extension name', extension.name);
        if (confirmation !== extension.name) { if (confirmation !== null) await showAlert('The extension name did not match. Nothing was removed.', 'Extension not dropped'); return; }
        try { await invoke('drop_extension', { id, name: extension.name }); await loadAdmin(id); }
        catch (error) { await showAlert(error?.message || 'Could not drop the extension', 'Extension not dropped'); }
      } else {
        if (!await showConfirm(`Install the extension “${extension.name}” in this database? Extension installation executes SQL supplied by its PostgreSQL package.`, 'Install extension', false, 'Install')) return;
        try { await invoke('install_extension', { id, name: extension.name }); await loadAdmin(id); }
        catch (error) { await showAlert(error?.message || 'Could not install the extension', 'Extension not installed'); }
      }
    });
    row.append(detail, action); panel.append(row);
  }
  return panel;
}

function renderQueryStatsPanel(id, stats, sortBy = 'total') {
  const panel = document.createElement('section'); panel.className = 'data-panel query-stats-panel';
  const heading = document.createElement('div'); heading.className = 'query-stats-heading';
  const title = document.createElement('h3'); title.textContent = 'Query stats · pg_stat_statements';
  const controls = document.createElement('div'); controls.className = 'query-stats-controls';
  const sort = document.createElement('select'); sort.setAttribute('aria-label', 'Sort query statistics');
  for (const [value, label] of [['total', 'Total time'], ['calls', 'Calls'], ['mean', 'Mean time']]) { const option = document.createElement('option'); option.value = value; option.textContent = label; sort.append(option); }
  sort.value = sortBy; sort.disabled = !stats.installed || !stats.queries.length;
  const reset = document.createElement('button'); reset.className = 'button small danger'; reset.type = 'button'; reset.textContent = 'Reset stats'; reset.disabled = !stats.installed;
  reset.addEventListener('click', async () => {
    if (!await showConfirm('Reset all pg_stat_statements counters? This does not change database rows, but the collected performance history will be lost.', 'Reset query statistics', true, 'Reset')) return;
    try { await invoke('reset_query_stats', { id }); await loadAdmin(id); }
    catch (error) { await showAlert(error?.message || 'Could not reset query statistics', 'Statistics not reset'); }
  });
  controls.append(sort, reset); heading.append(title, controls); panel.append(heading);
  if (!stats.installed) { panel.append(errorState('pg_stat_statements is not installed', 'Install it from Extensions; PostgreSQL may also require shared_preload_libraries and a restart.')); return panel; }
  if (!stats.queries.length) { panel.append(errorState('No query statistics', 'The extension is active but has not collected entries for this database.')); return panel; }
  const sortKey = { total: 'total_exec_ms', calls: 'calls', mean: 'mean_exec_ms' }[sortBy] || 'total_exec_ms';
  const rows = [...stats.queries].sort((left, right) => Number(right[sortKey]) - Number(left[sortKey]));
  for (const queryStat of rows) {
    const row = document.createElement('div'); row.className = 'query-stat-item';
    const detail = document.createElement('div');
    const query = document.createElement('code'); query.textContent = queryStat.query; query.title = queryStat.query;
    const metadata = document.createElement('small'); metadata.textContent = `${queryStat.calls} calls · ${queryStat.mean_exec_ms.toFixed(1)} ms mean · ${queryStat.total_exec_ms.toFixed(1)} ms total · ${queryStat.rows} rows`;
    detail.append(query, metadata);
    const actions = document.createElement('div'); actions.className = 'query-stat-actions';
    const open = document.createElement('button'); open.className = 'button small'; open.type = 'button'; open.textContent = 'Open query'; open.addEventListener('click', () => openSqlInNewTab(queryStat.query, id));
    const analyze = document.createElement('button'); analyze.className = 'button small'; analyze.type = 'button'; analyze.textContent = 'Ask assistant'; analyze.addEventListener('click', () => void askAssistantAboutQueryStat(id, queryStat));
    actions.append(open, analyze); row.append(detail, actions); panel.append(row);
  }
  sort.addEventListener('change', () => panel.replaceWith(renderQueryStatsPanel(id, stats, sort.value)));
  return panel;
}

async function loadAdmin(id) {
  const content = byId('admin-content'); content.replaceChildren();
  if (!id) { content.append(errorState('Choose a connected connection', 'Activity and locks are read from PostgreSQL.')); return; }
  content.append(errorState('Loading administration…', ''));
  const [adminResult, rolesResult, cronResult, extensionsResult, queryStatsResult] = await Promise.allSettled([invoke('admin', { id }), invoke('list_roles', { id }), invoke('list_cron_jobs', { id }), invoke('list_extensions', { id }), invoke('query_stats', { id })]);
  if (byId('admin-connection').value !== id) return;
  content.replaceChildren();
  content.append(rolesResult.status === 'fulfilled' ? renderRolesPanel(id, rolesResult.value) : unavailablePanel('Roles unavailable', 'The connected role may not have permission to inspect PostgreSQL roles.'));
  content.append(cronResult.status === 'fulfilled' ? renderCronJobsPanel(id, cronResult.value) : unavailablePanel('Scheduled jobs unavailable', 'Check pg_cron permissions and configuration.'));
  content.append(extensionsResult.status === 'fulfilled' ? renderExtensionsPanel(id, extensionsResult.value) : unavailablePanel('Extensions unavailable', 'The connected role may not have permission to inspect extensions.'));
  content.append(queryStatsResult.status === 'fulfilled' ? renderQueryStatsPanel(id, queryStatsResult.value) : unavailablePanel('Query statistics unavailable', 'Check pg_stat_statements configuration and monitoring permissions.'));
  if (adminResult.status === 'fulfilled') {
    const payload = adminResult.value;
    content.append(renderActivityPanel(id, payload.activity || []));
    content.append(dataPanel('Locks', (payload.locks || []).slice(0, 12).map((row) => [`${row.blocked_pid} → ${row.blocking_pid}`, row.wait_sec ? `${row.wait_sec}s` : 'waiting'])));
  } else {
    content.append(unavailablePanel('Activity and locks unavailable', 'Check monitoring permissions and reconnect.'));
  }
}

async function openTable(id, schema, table) {
  switchView('table-detail'); byId('detail-title').textContent = table; byId('detail-eyebrow').textContent = `${schema.toUpperCase()} · TABLE DETAIL`; byId('detail-summary').textContent = 'Loading';
  const content = byId('detail-content'); content.replaceChildren(errorState('Loading table detail…', ''));
  try {
    const payload = await invoke('table_detail', { id, schema, table }); const detail = payload.detail; content.replaceChildren(); byId('detail-summary').textContent = `${detail.row_estimate ?? 0} estimated rows`;
    content.append(tableMaintenancePanel(id, schema, table));
    content.append(dataPanel('Columns', (detail.columns || []).map((row) => [row.name, `${row.full_type || row.data_type}${row.is_nullable ? '' : ' · NOT NULL'}${row.is_primary_key ? ' · PK' : ''}`])));
    content.append(dataPanel('Constraints', (detail.constraints || []).map((row) => [row.name, `${row.type}: ${row.definition}`])));
    content.append(dataPanel('Indexes', (detail.indexes || []).map((row) => [row.name, row.definition])));
    content.append(dataPanel('Foreign keys', (detail.fk_map || []).map((row) => [row.constraint_name, `${row.direction} · ${row.foreign_table}.${row.foreign_column}`])));
    content.append(dataPanel('Column statistics', (payload.column_stats || []).map((row) => [row.column, `${row.null_frac == null ? '—' : `${(row.null_frac * 100).toFixed(1)}% null`} · ${row.n_distinct == null ? '—' : `${row.n_distinct} distinct`}`])));
    content.append(codePanel('DDL', payload.ddl));
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

const RESULT_ROW_HEIGHT = 35;
const RESULT_OVERSCAN = 10;
let resultVirtualCleanup = null;

function buildResultRow(result, rowIndex) {
  const row = result.rows[rowIndex];
  const tr = document.createElement('tr');
  tr.dataset.rowIndex = String(rowIndex);
  if (rowIndex % 2 === 1) tr.classList.add('result-row-alt');
  for (const column of result.columns) {
    const cell = document.createElement('td');
    const current = row[column];
    if (current === null || current === undefined) { cell.textContent = 'NULL'; cell.className = 'null'; }
    else if (typeof current === 'object') cell.textContent = JSON.stringify(current);
    else cell.textContent = String(current);
    tr.append(cell);
  }
  const actionCell = document.createElement('td');
  const detail = document.createElement('button');
  detail.className = 'button small row-detail-action';
  detail.type = 'button';
  detail.textContent = 'Details';
  detail.setAttribute('aria-label', `View details for row ${rowIndex + 1}`);
  detail.addEventListener('click', () => {
    void showAlert(resultRowToText(row, result.columns), `Row ${rowIndex + 1}`);
  });
  actionCell.append(detail);
  tr.append(actionCell);
  return tr;
}

/** Windowed rendering: only the rows inside (plus a small overscan around) the visible viewport
 * ever exist as DOM nodes. Two spacer <tr>s (sized in pixels, matching the rows they stand in for)
 * keep the scrollbar accurate for results with hundreds of thousands of rows. Returns a cleanup
 * function that removes the scroll listener — callers must invoke it before replacing the grid. */
function renderVirtualizedRows(tbody, result, columnCount) {
  const grid = byId('result-grid');
  const rowCount = result.rows.length;
  const topSpacer = document.createElement('tr');
  topSpacer.className = 'result-spacer';
  const topCell = document.createElement('td');
  topCell.colSpan = columnCount;
  topSpacer.append(topCell);
  const bottomSpacer = document.createElement('tr');
  bottomSpacer.className = 'result-spacer';
  const bottomCell = document.createElement('td');
  bottomCell.colSpan = columnCount;
  bottomSpacer.append(bottomCell);
  tbody.append(topSpacer, bottomSpacer);

  const update = () => {
    const { start, end } = visibleRange({
      rowCount,
      rowHeight: RESULT_ROW_HEIGHT,
      scrollTop: grid.scrollTop,
      viewportHeight: grid.clientHeight,
      overscan: RESULT_OVERSCAN,
    });
    topCell.style.height = `${start * RESULT_ROW_HEIGHT}px`;
    bottomCell.style.height = `${(rowCount - end) * RESULT_ROW_HEIGHT}px`;
    for (const row of tbody.querySelectorAll('tr[data-row-index]')) row.remove();
    const fragment = document.createDocumentFragment();
    for (let index = start; index < end; index += 1) fragment.append(buildResultRow(result, index));
    tbody.insertBefore(fragment, bottomSpacer);
  };

  let frame = null;
  const onScroll = () => {
    if (frame !== null) return;
    frame = requestAnimationFrame(() => { frame = null; update(); });
  };
  grid.addEventListener('scroll', onScroll);
  update();
  return () => grid.removeEventListener('scroll', onScroll);
}

function renderResult(result) {
  state.result = result;
  if (resultVirtualCleanup) { resultVirtualCleanup(); resultVirtualCleanup = null; }
  const grid = byId('result-grid');
  const error = byId('result-error');
  error.textContent = '';
  byId('export-csv').disabled = !result;
  byId('export-json').disabled = !result;
  byId('copy-result').disabled = !result;
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
  const actionHeader = document.createElement('th');
  actionHeader.textContent = 'Row';
  headerRow.append(actionHeader);
  head.append(headerRow);
  const body = document.createElement('tbody');
  table.append(head, body);
  grid.append(table);
  resultVirtualCleanup = renderVirtualizedRows(body, result, result.columns.length + 1);
}

async function runQuery(mode = 'query') {
  const script = mode === 'script';
  const explain = mode === 'explain';
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
  byId('explain-query').disabled = true;
  byId('cancel-query').hidden = false;
  byId('query-status').textContent = explain ? 'Planning query…' : script ? 'Running script…' : selectionActive ? 'Running selection…' : 'Running query…';
  byId('result-error').textContent = '';
  try {
    const command = explain ? 'execute_explain' : script ? 'execute_script' : 'execute_query';
    const result = await invoke(command, { id, sql, operationId: state.currentQueryOperationId });
    renderResult(result);
    byId('query-status').textContent = explain ? 'Plan ready' : 'Completed';
  } catch (error) {
    renderResult(null);
    byId('result-error').textContent = state.cancelRequested ? 'Query cancelled.' : explain ? 'EXPLAIN failed. Check the connection and SQL.' : 'Query failed. Check the connection and SQL.';
    byId('query-status').textContent = state.cancelRequested ? 'Cancelled' : 'Error';
  } finally {
    state.currentQueryId = null;
    state.currentQueryOperationId = null;
    state.cancelRequested = false;
    byId('run-query').disabled = false;
    byId('run-script').disabled = false;
    byId('explain-query').disabled = false;
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
  const { content, type, extension } = serializeResult(state.result, format);
  const url = URL.createObjectURL(new Blob([content], { type }));
  const link = document.createElement('a');
  link.href = url; link.download = `draco-result.${extension}`; link.click(); URL.revokeObjectURL(url);
}

async function copyResult() {
  if (!state.result) return;
  try {
    if (!navigator.clipboard?.writeText) throw new Error('Clipboard API unavailable');
    await navigator.clipboard.writeText(resultToTsv(state.result));
    byId('query-status').textContent = 'Result copied';
  } catch {
    await showAlert('The result could not be copied. Check clipboard permission for the Draco window.', 'Copy unavailable');
  }
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
    item.append(sql, meta, remove); item.addEventListener('click', () => { setEditorValue(entry.sql); switchView('query'); }); list.append(item);
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
    const actions = document.createElement('div'); actions.className = 'snippet-meta snippet-actions';
    const rename = document.createElement('button'); rename.className = 'button small'; rename.type = 'button'; rename.textContent = 'Rename';
    rename.addEventListener('click', async (event) => {
      event.stopPropagation();
      const nextName = await showPrompt('Choose a new name for this snippet.', 'Rename snippet', 'Snippet name', '', snippet.name);
      if (!nextName || nextName === snippet.name) return;
      try { await invoke('rename_snippet', { id: snippet.id, name: nextName }); await loadSnippets(); }
      catch (error) { await showAlert(error?.message || 'Could not rename snippet', 'Snippet not renamed'); }
    });
    const remove = document.createElement('button'); remove.className = 'button small danger'; remove.type = 'button'; remove.textContent = 'Delete';
    remove.addEventListener('click', async (event) => {
      event.stopPropagation();
      if (!await showConfirm(`Delete the snippet “${snippet.name}”?`, 'Delete snippet', true)) return;
      try { await invoke('delete_snippet', { id: snippet.id }); await loadSnippets(); }
      catch (error) { await showAlert(error?.message || 'Could not delete snippet', 'Snippet not deleted'); }
    });
    actions.append(rename, remove);
    item.append(name, sql, meta, actions); item.addEventListener('click', () => { setEditorValue(snippet.sql); switchView('query'); }); list.append(item);
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

function explorerSection(children, label) {
  const heading = document.createElement('div');
  heading.className = 'tree-section-label';
  heading.textContent = label;
  children.append(heading);
}

function renderExplorerTables(id, schemaName, children, tables, objects = []) {
  children.replaceChildren();
  if (tables.length) explorerSection(children, 'Tables & views');
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
  const groups = [
    ['Functions & procedures', ['function', 'procedure'], { function: 'ƒ', procedure: '⚙' }],
    ['Sequences', ['sequence'], { sequence: '↗' }],
    ['Triggers', ['trigger'], { trigger: '⚡' }],
  ];
  for (const [label, kinds, icons] of groups) {
    const matching = objects.filter((object) => kinds.includes(object.kind));
    if (!matching.length) continue;
    explorerSection(children, label);
    for (const object of matching) {
      const item = document.createElement('div');
      item.className = 'tree-item schema-object';
      const name = document.createElement('span');
      name.className = 'tree-item-label';
      name.textContent = `${icons[object.kind]} ${object.name}`;
      const detail = document.createElement('small');
      detail.textContent = object.detail || object.kind;
      item.append(name, detail);
      children.append(item);
    }
  }
  if (!tables.length && !objects.length) {
    const empty = document.createElement('div');
    empty.className = 'empty-state';
    empty.textContent = 'No objects in this schema';
    children.append(empty);
  }
}

async function loadExplorerTablesForFilter(id) {
  const request = ++state.explorerFilterRequest;
  const groups = [...byId('explorer-tree').querySelectorAll('.tree-group')];
  await Promise.all(groups.filter((group) => group.dataset.loaded !== 'true').map(async (group) => {
    const schemaName = group.dataset.schema;
    if (!schemaName) return;
    try {
      const [tables, objects] = await Promise.all([
        invoke('list_tables', { id, schema: schemaName }),
        invoke('list_schema_objects', { id, schema: schemaName }),
      ]);
      if (request !== state.explorerFilterRequest) return;
      renderExplorerTables(id, schemaName, group.querySelector('.tree-children'), tables, objects);
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
        const [tables, objects] = await Promise.all([
          invoke('list_tables', { id, schema: schema.name }),
          invoke('list_schema_objects', { id, schema: schema.name }),
        ]);
        renderExplorerTables(id, schema.name, children, tables, objects);
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
  for (const view of ['connections', 'explorer', 'dashboard', 'admin', 'backup', 'assistant', 'query', 'history', 'snippets', 'preferences', 'table-detail', 'erd']) byId(`view-${view}`).hidden = view !== name;
  byId('page-title').textContent = name === 'preferences' ? 'Preferences' : name[0].toUpperCase() + name.slice(1);
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
  ['Preferences', 'Theme, colors, updates and about Draco', 'preferences'],
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

function showDialog({ title, message, eyebrow = 'DRACO', kind = 'confirm', confirmLabel = 'Confirm', cancelLabel = 'Cancel', inputLabel = 'Value', placeholder = '', inputValue = '' }) {
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
  confirm.classList.toggle('danger-action', kind === 'confirm-danger' || kind === 'prompt-danger');
  inputWrap.hidden = kind !== 'prompt' && kind !== 'prompt-danger';
  byId('app-dialog-input-label').textContent = inputLabel;
  byId('app-dialog-input').placeholder = placeholder;
  byId('app-dialog-input').value = inputValue;
  cancel.hidden = kind === 'alert';
  dialog.hidden = false;
  const result = new Promise((resolve) => { dialogResolver = resolve; });
  const focusTarget = kind === 'prompt' || kind === 'prompt-danger' ? byId('app-dialog-input') : confirm;
  window.setTimeout(() => focusTarget.focus(), 0);
  return result;
}

function showAlert(message, title = 'Notice') {
  return showDialog({ title, message, kind: 'alert', confirmLabel: 'OK' });
}

function showConfirm(message, title = 'Confirm action', danger = false, confirmLabel = danger ? 'Delete' : 'Confirm') {
  return showDialog({ title, message, kind: danger ? 'confirm-danger' : 'confirm', confirmLabel });
}

function showPrompt(message, title = 'Enter a value', inputLabel = 'Value', placeholder = '', inputValue = '') {
  return showDialog({ title, message, kind: 'prompt', confirmLabel: 'Save', inputLabel, placeholder, inputValue }).then((accepted) => accepted === null || accepted === false ? null : String(accepted).trim());
}

function showDangerPrompt(message, title, inputLabel, placeholder = '') {
  return showDialog({ title, message, kind: 'prompt-danger', confirmLabel: 'Delete', inputLabel, placeholder }).then((accepted) => accepted === null || accepted === false ? null : String(accepted).trim());
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
  const preferences = await loadPreferences();
  try {
    if (!invoke) throw new Error('Tauri IPC is unavailable');
    const health = await invoke('health');
    byId('health-label').textContent = health.ready ? 'Backend ready' : 'Backend unavailable';
    byId('health-value').textContent = health.ready ? 'Bridge online' : 'Bridge offline';
    await refreshConnections();
    if (preferences.check_updates_on_startup) void checkForUpdates(false);
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
  if (event.key === 'F8') { event.preventDefault(); switchView('query'); runQuery(); }
  if (event.key === 'F10') { event.preventDefault(); switchView('query'); runQuery('explain'); }
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
  syncEditorHighlight();
  saveCurrentQueryTab();
  byId('query-status').textContent = 'SQL formatted';
});
byId('run-query').addEventListener('click', () => runQuery());
byId('run-script').addEventListener('click', () => runQuery('script'));
byId('explain-query').addEventListener('click', () => runQuery('explain'));
byId('cancel-query').addEventListener('click', cancelQuery);
byId('export-csv').addEventListener('click', () => downloadResult('csv'));
byId('export-json').addEventListener('click', () => downloadResult('json'));
byId('copy-result').addEventListener('click', copyResult);
byId('save-current-snippet').addEventListener('click', saveCurrentSnippet);
byId('clear-history').addEventListener('click', async () => { if (await showConfirm('Clear all saved query history?', 'Clear history', true, 'Clear')) { await invoke('clear_history'); loadHistory(); } });
byId('sql-editor').addEventListener('keydown', (event) => {
  const popupOpen = !byId('sql-autocomplete').hidden;
  if (popupOpen && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
    event.preventDefault();
    setActiveSuggestion((autocompleteActive + (event.key === 'ArrowDown' ? 1 : -1) + autocompleteItems.length) % autocompleteItems.length);
    return;
  }
  if (popupOpen && (event.key === 'Enter' || event.key === 'Tab')) { event.preventDefault(); acceptSuggestion(); return; }
  if (popupOpen && event.key === 'Escape') { event.preventDefault(); hideAutocomplete(); return; }
  if (popupOpen && ['ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'].includes(event.key)) hideAutocomplete();
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') { event.preventDefault(); runQuery(); }
});
byId('sql-editor').addEventListener('input', () => { syncEditorHighlight(); void updateAutocomplete(); });
byId('sql-editor').addEventListener('click', hideAutocomplete);
byId('sql-editor').addEventListener('scroll', () => {
  const overlay = byId('sql-editor-highlight');
  overlay.scrollTop = byId('sql-editor').scrollTop;
  overlay.scrollLeft = byId('sql-editor').scrollLeft;
  hideAutocomplete();
});
byId('query-connection').addEventListener('change', () => { void completionIndexFor(byId('query-connection').value); });
document.addEventListener('click', (event) => {
  if (!byId('sql-autocomplete').hidden && !event.target.closest('.sql-editor-wrap')) hideAutocomplete();
});
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
for (const button of document.querySelectorAll('[data-preference-section]')) button.addEventListener('click', () => showPreferenceSection(button.dataset.preferenceSection));
for (const button of document.querySelectorAll('[data-theme-choice]')) button.addEventListener('click', () => void savePreferences({ theme: button.dataset.themeChoice }));
for (const button of document.querySelectorAll('[data-accent-choice]')) button.addEventListener('click', () => void savePreferences({ accent: button.dataset.accentChoice }));
byId('check-updates-startup').addEventListener('change', (event) => void savePreferences({ check_updates_on_startup: event.target.checked }));
byId('check-updates').addEventListener('click', () => void checkForUpdates(true));
byId('copy-release-link').addEventListener('click', async () => { if (!state.releaseUrl) return; await navigator.clipboard.writeText(state.releaseUrl); byId('update-detail').textContent = 'Release link copied'; });
byId('copy-pix-key').addEventListener('click', async () => { await navigator.clipboard.writeText(PIX_KEY); byId('pix-status').textContent = 'Pix key copied'; });
byId('copy-pix-code').addEventListener('click', async () => { await navigator.clipboard.writeText(PIX_COPY_AND_PASTE); byId('pix-status').textContent = 'Pix copy-and-paste code copied'; });
for (const button of document.querySelectorAll('[data-view]')) button.addEventListener('click', () => switchView(button.dataset.view));
bindWindowControls();
bindSidebarToggle();
bindAppDialog();
renderQueryTabs();
syncEditorHighlight();
applyAppearance(state.preferences);
boot();
