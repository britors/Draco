import { resultRowToText, resultToTsv, serializeResult } from './result-export.js';
import { highlightSqlIncremental } from './sql-highlight.js';
import { applySuggestion, buildCompletionIndex, suggest } from './sql-autocomplete.js';
import { visibleRange } from './virtual-list.js';
import { groupSchemaObjects, schemaObjectSql } from './explorer-navigation.js';
import { clampResultColumnWidth } from './result-columns.js';
import { AI_QUERY_REVIEW_FOCUSES, buildAiQueryReviewMessage } from './ai-query-review.js';
import { assembleFunctionDdl, formatFunctionParameters, parseFunctionParameters, sliceFunctionDdl } from './function-ddl.js';
import { applyTranslations, createTranslator, errorMessage, resolveLocale } from './i18n.js';
import { sqlMayWrite } from './sql-write-detect.js';
import { formatLagSeconds, formatWalBytes, replicationMode, slotNeedsAttention } from './replication-view.js';

const tauriInvoke = window.__TAURI__?.core?.invoke;
// Commands that change the database, all keyed by the connection `id`. The backend refuses them
// on read-only connections; here they fail early and, on production, wait for the connection name.
const WRITE_COMMANDS = new Set([
  'alter_table', 'create_cron_job', 'create_role', 'create_schema', 'create_sequence', 'create_table',
  'create_trigger', 'delete_cron_job', 'delete_role', 'delete_routine', 'delete_table_row', 'delete_trigger',
  'drop_extension', 'insert_table_row', 'install_extension', 'next_sequence_value', 'reset_query_stats',
  'run_restore', 'run_routine', 'run_table_import', 'run_table_maintenance', 'save_function_definition', 'save_index_definition',
  'save_sequence_definition', 'save_trigger_definition', 'save_view_definition', 'set_cron_job_active',
  'set_sequence_value', 'update_cron_job', 'update_role', 'update_table_cell',
]);
// Free-form SQL is only confirmed when the heuristic sees a possible write; on read-only
// connections PostgreSQL itself rejects the write, so no confirmation is asked there.
const SQL_COMMANDS = new Set(['execute_query', 'execute_script']);
const invoke = tauriInvoke ? guardedInvoke : null;

async function guardedInvoke(command, args = {}) {
  const connection = typeof args?.id === 'string' ? state.connections.find((item) => item.id === args.id) : null;
  if (connection && WRITE_COMMANDS.has(command)) {
    if (connection.read_only) throw { code: 'connection_read_only', key: 'error.connection_read_only' };
    if (connection.environment === 'production') await confirmProductionWrite(connection);
  } else if (connection && SQL_COMMANDS.has(command) && connection.environment === 'production' && !connection.read_only && sqlMayWrite(args.sql)) {
    await confirmProductionWrite(connection);
  }
  return tauriInvoke(command, args);
}

async function confirmProductionWrite(connection) {
  const typed = await showDialog({
    title: t('production.writeTitle'),
    message: t('production.writeConfirm', { name: connection.label }),
    kind: 'prompt-danger',
    confirmLabel: t('production.writeAction'),
    inputLabel: t('production.connectionName'),
    placeholder: connection.label,
  });
  if (typeof typed !== 'string' || typed.trim() !== connection.label) throw { code: 'write_cancelled', key: 'error.write_cancelled' };
}
const currentWindow = window.__TAURI__?.window?.getCurrentWindow?.();
// The interface follows the system locale reported by the webview; English is the fallback.
const t = createTranslator(resolveLocale(navigator.languages?.length ? navigator.languages : [navigator.language]));
document.documentElement.lang = t.locale;
applyTranslations(document, t);
const state = { connections: [], selectedConnectionId: null, selectedSchema: null, explorerFilter: '', explorerFilterRequest: 0, explorerConnectionRequest: 0, lastTested: null, result: null, currentQueryId: null, currentQueryOperationId: null, cancelRequested: false, currentOperationId: null, preferences: { version: '2.1.7', theme: 'dark', accent: 'coral', check_updates_on_startup: true, programming_workspace: null, notify_long_operations: true }, releaseUrl: '', queryTabs: [{ id: 1, label: t('query.tabLabel', { number: 1 }), sql: '' }], currentQueryTabId: 1 };
let dialogResolver = null;
let aiReviewRequest = null;
let aiReviewReturnFocus = null;
let aiReviewFocus = 'general';
// Set right before switchView('assistant') by every "jump to the Assistant from elsewhere"
// entry point (Review with AI from the SQL/Programming editors, "Ask assistant" on a query
// stat) so the Assistant page can offer a way back to wherever the user actually came from.
// Cleared whenever the user navigates to Assistant directly via the sidebar/topbar nav instead.
let assistantReturnView = null;
const ASSISTANT_BACK_LABELS = { query: t('assistant.backToSqlEditor'), programming: t('assistant.backToProgramming'), admin: t('assistant.backToAdministration') };

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
  byId('notify-long-operations').checked = state.preferences.notify_long_operations !== false;
  byId('about-version').textContent = state.preferences.version;
  byId('update-detail').textContent = t('updates.currentVersion', { version: state.preferences.version });
  const workspacePreference = byId('programming-workspace-preference');
  if (workspacePreference) workspacePreference.value = state.preferences.programming_workspace || '';
}

async function loadPreferences() {
  try {
    state.preferences = await invoke('preferences');
    programmingWorkspacePath = state.preferences.programming_workspace || null;
    syncPreferenceControls();
    if (programmingWorkspacePath) void loadProgrammingLocalFiles();
    return state.preferences;
  } catch {
    applyAppearance(state.preferences);
    byId('preferences-status').textContent = t('prefs.couldNotLoadPreferences');
    return state.preferences;
  }
}

async function savePreferences(patch) {
  state.preferences = { ...state.preferences, ...patch };
  syncPreferenceControls();
  byId('preferences-status').textContent = t('prefs.saving');
  try {
    state.preferences = await invoke('save_preferences', { preferences: state.preferences });
    syncPreferenceControls();
    byId('preferences-status').textContent = t('prefs.savedLocally');
  } catch {
    byId('preferences-status').textContent = t('prefs.couldNotSave');
  }
}

let assistantSettingsDraft = null;
let activeAssistantProvider = 'anthropic';
let assistantModelsRequest = 0;

function assistantModelField(provider) {
  return `${provider}_model`;
}

function assistantProviderLabel(provider) {
  return { anthropic: 'Anthropic', openai: 'OpenAI', gemini: 'Gemini' }[provider] || provider;
}

function renderAssistantSettings(settings) {
  assistantSettingsDraft = { ...settings };
  activeAssistantProvider = settings.provider;
  byId('ai-provider').value = settings.provider;
  byId('ai-model').value = settings[assistantModelField(settings.provider)] || '';
  byId('ai-daily-limit').value = String(settings.max_messages_per_day);
  byId('ai-round-limit').value = String(settings.max_rounds_per_message);
  byId('ai-key-title').textContent = t('aiSettings.credentialTitle', { provider: assistantProviderLabel(settings.provider) });
}

async function loadAssistantModels(provider = byId('ai-provider').value) {
  const request = ++assistantModelsRequest;
  const status = byId('ai-settings-status');
  const refresh = byId('refresh-ai-models');
  refresh.disabled = true;
  status.textContent = t('aiSettings.loadingModels', { provider: assistantProviderLabel(provider) });
  status.className = 'form-status';
  try {
    const models = await invoke('assistant_models', { provider });
    if (request !== assistantModelsRequest || byId('ai-provider').value !== provider) return;
    const list = byId('ai-model-list');
    list.replaceChildren();
    for (const model of models) {
      const option = document.createElement('option');
      option.value = model;
      list.append(option);
    }
    status.textContent = t('aiSettings.modelsAvailable', { count: models.length, provider: assistantProviderLabel(provider) });
    status.className = 'form-status success';
  } catch (error) {
    if (request !== assistantModelsRequest || byId('ai-provider').value !== provider) return;
    byId('ai-model-list').replaceChildren();
    status.textContent = (error?.key ? errorMessage(error, t) : error?.message || t('aiSettings.modelsFailed', { provider: assistantProviderLabel(provider) }));
    status.className = 'form-status error';
  } finally {
    if (request === assistantModelsRequest) refresh.disabled = false;
  }
}

async function loadAssistantSettings() {
  const status = byId('ai-settings-status');
  status.textContent = t('aiSettings.loadingAiSettings');
  status.className = 'form-status';
  try {
    renderAssistantSettings(await invoke('assistant_settings'));
    status.textContent = t('aiSettings.apiKeysRemainProtectedBy');
    void loadAssistantModels(activeAssistantProvider);
  } catch (error) {
    status.textContent = errorMessage(error, t, 'aiSettings.couldNotLoadAiSettings');
    status.className = 'form-status error';
  }
}

function changeAssistantProvider(provider) {
  if (!assistantSettingsDraft) return;
  assistantSettingsDraft[assistantModelField(activeAssistantProvider)] = value('ai-model');
  activeAssistantProvider = provider;
  assistantSettingsDraft.provider = provider;
  byId('ai-model').value = assistantSettingsDraft[assistantModelField(provider)] || '';
  byId('ai-api-key').value = '';
  byId('ai-key-title').textContent = t('aiSettings.credentialTitle', { provider: assistantProviderLabel(provider) });
  byId('ai-settings-status').textContent = t('aiSettings.configure', { provider: assistantProviderLabel(provider) });
  byId('ai-settings-status').className = 'form-status';
  void loadAssistantModels(provider);
}

async function saveAssistantSettings(event) {
  event.preventDefault();
  if (!assistantSettingsDraft) return;
  const model = value('ai-model');
  const dailyLimit = Number(value('ai-daily-limit'));
  const roundLimit = Number(value('ai-round-limit'));
  const status = byId('ai-settings-status');
  if (!model || !Number.isInteger(dailyLimit) || dailyLimit < 1 || dailyLimit > 10000 || !Number.isInteger(roundLimit) || roundLimit < 1 || roundLimit > 32) {
    status.textContent = t('aiSettings.enterAModelADaily');
    status.className = 'form-status error';
    return;
  }
  assistantSettingsDraft.provider = activeAssistantProvider;
  assistantSettingsDraft[assistantModelField(activeAssistantProvider)] = model;
  assistantSettingsDraft.max_messages_per_day = dailyLimit;
  assistantSettingsDraft.max_rounds_per_message = roundLimit;
  byId('save-ai-settings').disabled = true;
  status.textContent = t('aiSettings.savingAiSettings');
  status.className = 'form-status';
  try {
    renderAssistantSettings(await invoke('save_assistant_settings', { settings: assistantSettingsDraft }));
    status.textContent = t('aiSettings.aiSettingsSavedLocally');
    status.className = 'form-status success';
  } catch (error) {
    status.textContent = errorMessage(error, t, 'aiSettings.couldNotSaveAiSettings');
    status.className = 'form-status error';
  } finally {
    byId('save-ai-settings').disabled = false;
  }
}

async function saveAssistantKey() {
  const provider = byId('ai-provider').value;
  const key = value('ai-api-key');
  const status = byId('ai-settings-status');
  if (!key) { status.textContent = t('aiSettings.pasteAnApiKeyBefore'); status.className = 'form-status error'; return; }
  byId('save-ai-key').disabled = true;
  status.textContent = t('aiSettings.savingKey', { provider: assistantProviderLabel(provider) });
  status.className = 'form-status';
  try {
    await invoke('save_assistant_key', { provider, key });
    byId('ai-api-key').value = '';
    status.textContent = t('aiSettings.keySaved', { provider: assistantProviderLabel(provider) });
    status.className = 'form-status success';
    void loadAssistantModels(provider);
  } catch (error) {
    status.textContent = errorMessage(error, t, 'aiSettings.couldNotSaveTheApi');
    status.className = 'form-status error';
  } finally {
    byId('save-ai-key').disabled = false;
  }
}

async function clearAssistantKey() {
  const provider = byId('ai-provider').value;
  if (!await showConfirm(t('aiSettings.removeKeyConfirm', { provider: assistantProviderLabel(provider) }), t('aiSettings.removeAiCredential'), true, t('aiSettings.removeKey'))) return;
  const status = byId('ai-settings-status');
  byId('clear-ai-key').disabled = true;
  try {
    await invoke('clear_assistant_key', { provider });
    byId('ai-api-key').value = '';
    ++assistantModelsRequest;
    byId('ai-model-list').replaceChildren();
    status.textContent = t('aiSettings.keyRemoved', { provider: assistantProviderLabel(provider) });
    status.className = 'form-status success';
  } catch (error) {
    status.textContent = errorMessage(error, t, 'aiSettings.couldNotRemoveTheApi');
    status.className = 'form-status error';
  } finally {
    byId('clear-ai-key').disabled = false;
  }
}

let githubConnection = null;
let githubBranches = [];
let githubRepositories = [];
let pendingGithubCommit = null;
// Kept as a compatibility marker for older bridge versions that still expose connect_github.
const legacyGithubConnectCommand = 'connect_github';

function renderGithubConnection(connection) {
  githubConnection = connection;
  byId('github-owner').value = connection.owner || '';
  byId('github-repository').value = connection.repository || '';
  byId('github-root-path').value = connection.root_path || '';
  byId('github-default-branch').value = connection.default_branch || '';
  byId('github-disconnect').disabled = !connection.connected;
  const status = byId('github-settings-status');
  status.textContent = connection.connected
    ? `${t('githubSettings.connectedAs', { login: connection.account_login })}${connection.owner && connection.repository ? ` · ${connection.owner}/${connection.repository}` : ''}`
    : t('githubSettings.githubIsNotConnected');
  status.className = `form-status ${connection.connected ? 'success' : ''}`.trim();
}

async function loadGithubSettings() {
  const status = byId('github-settings-status');
  status.textContent = t('githubSettings.checkingGithubConnection');
  status.className = 'form-status';
  try {
    renderGithubConnection(await invoke('github_status'));
  } catch (error) {
    status.textContent = errorMessage(error, t, 'githubSettings.couldNotCheckTheGithub');
    status.className = 'form-status error';
  }
}

async function connectGithub(event) {
  event.preventDefault();
  const token = value('github-token');
  const status = byId('github-settings-status');
  if (!token) {
    status.textContent = t('githubSettings.pasteAGithubTokenBefore');
    status.className = 'form-status error';
    return;
  }
  byId('github-connect').disabled = true;
  status.textContent = t('githubSettings.validatingCredentialWithGithub');
  status.className = 'form-status';
  try {
    let connection;
    try {
      connection = await invoke('connect_github_token', { token });
    } catch (tokenError) {
      // Older installed binaries may not have the credential-only command yet.
      connection = await invoke('connect_github', { settings: { owner: value('github-owner'), repository: value('github-repository'), root_path: value('github-root-path'), default_branch: value('github-default-branch') || 'main' }, token });
      if (!connection) throw tokenError;
    }
    renderGithubConnection(connection);
    byId('github-token').value = '';
    await loadProgrammingGithub();
  } catch (error) {
    status.textContent = errorMessage(error, t, 'githubSettings.couldNotConnectGithub');
    status.className = 'form-status error';
  } finally {
    byId('github-connect').disabled = false;
  }
}

async function disconnectGithub() {
  if (!await showConfirm(t('githubSettings.removeTheGithubTokenFrom'), t('githubSettings.disconnectGithub'), true, t('connections.disconnect'))) return;
  const status = byId('github-settings-status');
  byId('github-disconnect').disabled = true;
  try {
    await invoke('disconnect_github');
    renderGithubConnection({ ...(githubConnection || {}), connected: false, account_login: null });
    githubBranches = [];
    githubRepositories = [];
    renderProgrammingGithubBranches();
  } catch (error) {
    status.textContent = errorMessage(error, t, 'githubSettings.couldNotDisconnectGithub');
    status.className = 'form-status error';
  }
}

async function checkForUpdates(manual = true) {
  const button = byId('check-updates');
  button.disabled = true;
  byId('update-title').textContent = t('updates.checkingForUpdates');
  try {
    const update = await invoke('check_for_updates');
    state.releaseUrl = update.release_url;
    byId('copy-release-link').hidden = !update.update_available;
    byId('update-title').textContent = update.update_available ? t('updates.available', { version: update.latest_version }) : t('updates.dracoIsUpToDate');
    byId('update-detail').textContent = update.update_available ? t('updates.installedLatest', { current: update.current_version, latest: update.latest_version }) : t('updates.currentVersion', { version: update.current_version });
  } catch {
    byId('update-title').textContent = manual ? t('updates.couldNotCheckForUpdates') : t('updates.automaticUpdateCheckUnavailable');
    byId('update-detail').textContent = t('updates.checkYourNetworkConnectionAnd');
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
  if (name === 'ai') void loadAssistantSettings();
  if (name === 'github') void loadGithubSettings();
}

let sqlHighlightCache = [];

function syncEditorHighlight() {
  const editor = byId('sql-editor');
  const overlay = byId('sql-editor-highlight');
  const highlighted = highlightSqlIncremental(editor.value, sqlHighlightCache);
  sqlHighlightCache = highlighted.cache;
  overlay.innerHTML = highlighted.html;
  overlay.scrollTop = editor.scrollTop;
  overlay.scrollLeft = editor.scrollLeft;
}

function setEditorValue(text) {
  byId('sql-editor').value = text;
  syncEditorHighlight();
  hideAutocomplete();
}

let programmingHighlightCache = [];

function syncProgrammingEditorHighlight() {
  const editor = byId('programming-editor');
  const overlay = byId('programming-editor-highlight');
  const highlighted = highlightSqlIncremental(editor.value, programmingHighlightCache);
  programmingHighlightCache = highlighted.cache;
  overlay.innerHTML = highlighted.html;
  overlay.scrollTop = editor.scrollTop;
  overlay.scrollLeft = editor.scrollLeft;
}

function setProgrammingEditorValue(text) {
  programmingHighlightCache = [];
  byId('programming-editor').value = text;
  syncProgrammingEditorHighlight();
}

const completionCache = new Map();
let autocompleteItems = [];
let autocompleteActive = 0;
// Which editor/popup pair the suggestions currently on screen belong to — the SQL editor and
// the Programming body editor each have their own popup (they live in different, mutually
// hidden views) but share this same selection/accept machinery.
let autocompleteEditor = null;
let autocompletePopup = null;

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
  if (autocompletePopup) { autocompletePopup.hidden = true; autocompletePopup.replaceChildren(); }
  autocompleteItems = [];
  autocompleteEditor = null;
  autocompletePopup = null;
}

function setActiveSuggestion(index) {
  const popup = autocompletePopup;
  if (!popup) return;
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
  const editor = autocompleteEditor;
  if (!suggestion || !editor) return;
  const { text, caret } = applySuggestion(editor.value, editor.selectionStart, suggestion);
  editor.value = text;
  editor.setSelectionRange(caret, caret);
  const isProgrammingEditor = editor === byId('programming-editor');
  hideAutocomplete();
  if (isProgrammingEditor) { syncProgrammingEditorHighlight(); notifyProgrammingFormChanged(); }
  else { syncEditorHighlight(); saveCurrentQueryTab(); }
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

function renderAutocomplete(items, position, editor, popup) {
  autocompleteItems = items;
  autocompleteActive = 0;
  autocompleteEditor = editor;
  autocompletePopup = popup;
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
    detail.textContent = [item.detail, t(`autocomplete.${item.kind}`)].filter(Boolean).join(' · ');
    option.append(label, detail);
    option.addEventListener('mousedown', (event) => { event.preventDefault(); acceptSuggestion(index); });
    popup.append(option);
  });
  popup.hidden = false;
  popup.style.top = `${position.top}px`;
  popup.style.left = `${position.left}px`;
}

async function updateAutocompleteFor(editor, popup, connectionId) {
  if (editor.selectionStart !== editor.selectionEnd) { hideAutocomplete(); return; }
  const index = await completionIndexFor(connectionId);
  const suggestions = suggest(index, editor.value, editor.selectionStart);
  if (!suggestions.length) { hideAutocomplete(); return; }
  renderAutocomplete(suggestions, caretPixelPosition(editor, editor.selectionStart), editor, popup);
}

async function updateAutocomplete() {
  await updateAutocompleteFor(byId('sql-editor'), byId('sql-autocomplete'), byId('query-connection').value);
}

async function updateProgrammingAutocomplete() {
  await updateAutocompleteFor(byId('programming-editor'), byId('programming-sql-autocomplete'), programmingEditorTarget?.id);
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
    button.title = t('query.tabRenameHint');
    button.addEventListener('click', () => selectQueryTab(tab.id));
    button.addEventListener('dblclick', (event) => { event.preventDefault(); void renameQueryTab(tab.id); });
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'query-tab-close';
    close.textContent = '×';
    close.title = t('query.tabClose', { label: tab.label });
    close.setAttribute('aria-label', close.title);
    close.addEventListener('click', (event) => { event.stopPropagation(); closeQueryTab(tab.id); });
    wrap.append(button, close);
    tabs.append(wrap);
  }
  const add = document.createElement('button');
  add.type = 'button';
  add.className = 'query-tab-new';
  add.textContent = '+';
  add.title = t('query.newTab');
  add.setAttribute('aria-label', t('query.newTab'));
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
  state.queryTabs.push({ id: next, label: t('query.tabLabel', { number: next }), sql: '' });
  state.currentQueryTabId = next;
  setEditorValue('');
  renderQueryTabs();
  byId('sql-editor').focus();
}

function closeQueryTab(id) {
  const index = state.queryTabs.findIndex((item) => item.id === id);
  if (index === -1) return;
  if (state.queryTabs.length === 1) {
    state.queryTabs = [{ id, label: t('query.tabLabel', { number: 1 }), sql: '' }];
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
  const name = await showPrompt(t('query.renameTabMessage'), t('query.renameTabTitle'), t('query.tabName'), '', tab.label);
  if (!name) return;
  tab.label = name;
  renderQueryTabs();
}

function openSqlInNewTab(sql, connectionId, label = null) {
  switchView('query');
  showQueryWorkspaceSection('editor');
  newQueryTab();
  const tab = state.queryTabs.find((item) => item.id === state.currentQueryTabId);
  const firstLine = sql.split(/\r?\n/, 1)[0].trim();
  if (tab) {
    tab.sql = sql;
    tab.label = label || (firstLine.length > 28 ? `${firstLine.slice(0, 28)}…` : firstLine || tab.label);
  }
  setEditorValue(sql);
  byId('query-connection').value = connectionId; syncConnectionMarkers();
  renderQueryTabs();
}

async function saveCurrentSnippet() {
  const name = await showPrompt(t('snippets.saveMessage'), t('snippets.saveTitle'), t('snippets.name'), t('snippets.namePlaceholder'));
  const sql = byId('sql-editor').value;
  if (!name || !sql.trim()) return;
  try {
    await invoke('save_snippet', { input: { name, sql, conn_id: byId('query-connection').value || null } });
    byId('query-status').textContent = t('snippets.saved');
  } catch {
    byId('query-status').textContent = t('snippets.saveFailed');
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
    environment: byId('environment').value || null,
    read_only: byId('read-only').checked,
  };
}

const ENVIRONMENTS = ['development', 'staging', 'production'];

function connectionById(id) {
  return id ? state.connections.find((item) => item.id === id) || null : null;
}

function isReadOnly(id) {
  return Boolean(connectionById(id)?.read_only);
}

// Environment and read-only markers shown wherever a connection is named. The color comes from
// the --env-* tokens; the text keeps the meaning available without color.
function connectionBadges(connection) {
  const badges = [];
  if (ENVIRONMENTS.includes(connection?.environment)) {
    const badge = document.createElement('span');
    badge.className = 'env-badge';
    badge.dataset.environment = connection.environment;
    badge.textContent = t(`environment.${connection.environment}`);
    badges.push(badge);
  }
  if (connection?.read_only) {
    const badge = document.createElement('span');
    badge.className = 'env-badge read-only-badge';
    badge.textContent = t('environment.readOnly');
    badge.title = t('environment.readOnlyHint');
    badges.push(badge);
  }
  return badges;
}

function connectionOptionLabel(connection) {
  const markers = [];
  if (ENVIRONMENTS.includes(connection.environment)) markers.push(t(`environment.${connection.environment}`));
  if (connection.read_only) markers.push(t('environment.readOnly'));
  return markers.length ? `${connection.label} · ${markers.join(' · ')}` : connection.label;
}

const CONNECTION_SELECTS = ['query-connection', 'dashboard-connection', 'admin-connection', 'backup-connection', 'assistant-connection', 'programming-connection'];

// Mirrors the selected connection's environment on the select (and on the SQL editor) so the
// production color stays visible while working.
function syncConnectionMarkers() {
  for (const id of CONNECTION_SELECTS) {
    const select = byId(id);
    const connection = connectionById(select.value);
    select.dataset.environment = connection?.environment || '';
    select.toggleAttribute('data-read-only', Boolean(connection?.read_only));
  }
  const editor = byId('query-connection').closest('.query-workspace-panel');
  const queryConnection = connectionById(byId('query-connection').value);
  editor.dataset.environment = queryConnection?.environment || '';
  editor.toggleAttribute('data-read-only', Boolean(queryConnection?.read_only));
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
  byId('connection-url').value = '';
  byId('password').value = '';
  byId('ssh-password').value = '';
  byId('jump-password').value = '';
}

function renderConnections() {
  const list = byId('connection-list');
  list.replaceChildren();
  byId('connection-count').textContent = t('connections.count', { count: state.connections.length });
  if (!state.connections.length) {
    const empty = document.createElement('div');
    empty.className = 'empty-state';
    const title = document.createElement('strong');
    title.textContent = t('connections.emptyTitle');
    const hint = document.createElement('span');
    hint.textContent = t('connections.emptyHint');
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
    status.textContent = t(`connections.state.${connection.state}`);
    card.dataset.state = connection.state;
    if (connection.favorite) {
      const favorite = document.createElement('span');
      favorite.className = 'favorite-mark';
      favorite.textContent = '★';
      favorite.title = t('connections.favorite');
      favorite.setAttribute('aria-label', t('connections.favorite'));
      meta.append(dot, title, favorite, ...connectionBadges(connection), status);
    } else {
      meta.append(dot, title, ...connectionBadges(connection), status);
    }
    if (connection.environment) card.dataset.environment = connection.environment;
    const detail = document.createElement('div');
    detail.className = 'connection-detail';
    detail.textContent = `${connection.user}@${connection.host}:${connection.port}/${connection.database}${connection.ssh_enabled ? ' · SSH' : ''}`;
    const actions = document.createElement('div');
    actions.className = 'connection-actions';
    const connectButton = document.createElement('button');
    connectButton.className = `button small ${connection.state === 'connected' ? 'disconnect-action' : 'connect-action'}`;
    connectButton.type = 'button';
    connectButton.textContent = connection.state === 'connected' ? t('connections.disconnect') : t('connections.connect');
    connectButton.addEventListener('click', () => connection.state === 'connected' ? disconnect(connection.id) : connect(connection.id));
    const edit = document.createElement('button');
    edit.className = 'button small';
    edit.type = 'button';
    edit.textContent = t('connections.edit');
    edit.addEventListener('click', () => showForm(connection));
    const remove = document.createElement('button');
    remove.className = 'button small danger';
    remove.type = 'button';
    remove.textContent = t('connections.delete');
    remove.addEventListener('click', () => deleteConnection(connection.id));
    actions.append(connectButton, edit, remove);
    card.append(meta, actions, detail);
    list.append(card);
  }
}

function showForm(connection = null) {
  byId('connection-form').hidden = false;
  byId('form-title').textContent = connection ? t('connectionForm.editTitle') : t('connectionForm.newTitle');
  byId('connection-id').value = connection?.id ?? '';
  byId('label').value = connection?.label ?? '';
  byId('host').value = connection?.host ?? 'localhost';
  byId('port').value = connection?.port ?? 5432;
  byId('database').value = connection?.database ?? '';
  byId('user').value = connection?.user ?? '';
  byId('ssl').checked = connection?.ssl ?? false;
  byId('favorite').checked = connection?.favorite ?? false;
  byId('environment').value = connection?.environment ?? '';
  byId('read-only').checked = connection?.read_only ?? false;
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
  setStatus(connection ? t('connectionForm.hintExisting') : t('connectionForm.hintNew'), '');
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
    byId('dashboard-connection').value = id; syncConnectionMarkers();
    switchView('dashboard');
  } catch (error) {
    connection.state = 'error';
    connection.error = t('connections.connectionFailed');
    renderConnections();
  }
}

async function disconnect(id) {
  try {
    await invoke('disconnect', { id });
    await refreshConnections();
  } catch (error) {
    setStatus(error?.key ? errorMessage(error, t) : t('connections.disconnectFailed'), 'error');
  }
}

async function deleteConnection(id) {
  if (!await showConfirm(t('connections.deleteConfirm'), t('connections.deleteTitle'), true)) return;
  try {
    await invoke('delete_connection', { id });
    if (state.selectedConnectionId === id) state.selectedConnectionId = null;
    await refreshConnections();
  } catch (error) {
    setStatus(error?.key ? errorMessage(error, t) : t('connections.deleteFailed'), 'error');
  }
}

async function testCurrentConnection() {
  const request = connectionRequest();
  setStatus(t('connectionForm.testing'));
  byId('test-connection').disabled = true;
  try {
    await invoke('test_connection', { request });
    state.lastTested = JSON.stringify(request.input);
    setStatus(t('connectionForm.testSuccess'), 'success');
    return true;
  } catch (error) {
    state.lastTested = null;
    setStatus(t('connectionForm.testFailed'), 'error');
    return false;
  } finally {
    byId('test-connection').disabled = false;
  }
}

// The URL may carry a password: it only fills the transient password field, the URL field is
// cleared right away and saving still goes through the mandatory test.
async function fillFromConnectionUrl() {
  const field = byId('connection-url');
  const url = field.value.trim();
  if (!url) {
    setStatus(t('connectionUrl.empty'), 'error');
    return;
  }
  try {
    const parsed = await invoke('parse_connection_url', { url });
    field.value = '';
    byId('label').value = byId('label').value.trim() || parsed.label;
    byId('host').value = parsed.host;
    byId('port').value = parsed.port;
    byId('database').value = parsed.database;
    byId('user').value = parsed.user;
    byId('ssl').checked = parsed.ssl;
    if (parsed.password) byId('password').value = parsed.password;
    state.lastTested = null;
    const notes = [t('connectionUrl.filled')];
    if (parsed.ssl_verification_downgraded) notes.push(t('connectionUrl.sslNotVerified'));
    if (parsed.ignored_parameters.length) notes.push(t('connectionUrl.ignored', { names: parsed.ignored_parameters.join(', ') }));
    setStatus(notes.join(' '), 'success');
    (parsed.user ? byId('label') : byId('user')).focus();
  } catch (error) {
    setStatus(errorMessage(error, t), 'error');
  }
}

let connectionImportPreview = null;
let connectionImportReturnFocus = null;

async function openConnectionImport() {
  connectionImportReturnFocus = document.activeElement;
  byId('connection-import-dialog').hidden = false;
  byId('connection-import-reload').focus();
  await loadConnectionImport('preview_connection_imports');
}

async function closeConnectionImport() {
  byId('connection-import-dialog').hidden = true;
  connectionImportPreview = null;
  byId('connection-import-list').replaceChildren();
  try { await invoke('discard_connection_import'); } catch { /* nothing kept to discard */ }
  connectionImportReturnFocus?.focus?.();
}

async function loadConnectionImport(command, args = {}) {
  const status = byId('connection-import-status');
  status.textContent = t('connectionImport.reading');
  status.className = 'form-status';
  try {
    const preview = await invoke(command, args);
    if (!preview) {
      status.textContent = '';
      return;
    }
    connectionImportPreview = preview;
    renderConnectionImport();
    status.textContent = preview.sources.map((source) => source.found
      ? t(`connectionImport.source.${source.source}.found`, { count: source.count })
      : t(`connectionImport.source.${source.source}.missing`)).join(' · ');
  } catch (error) {
    status.textContent = errorMessage(error, t);
    status.className = 'form-status error';
  }
}

function renderConnectionImport(results = new Map()) {
  const list = byId('connection-import-list');
  list.replaceChildren();
  const candidates = connectionImportPreview?.candidates ?? [];
  if (!candidates.length) {
    const empty = document.createElement('div');
    empty.className = 'empty-state';
    empty.textContent = t('connectionImport.empty');
    list.append(empty);
  }
  for (const candidate of candidates) {
    const row = document.createElement('label');
    row.className = 'connection-import-row';
    const check = document.createElement('input');
    check.type = 'checkbox';
    check.value = candidate.key;
    const result = results.get(candidate.key);
    check.checked = !candidate.already_saved && result?.status !== 'saved';
    check.disabled = result?.status === 'saved';
    const text = document.createElement('span');
    text.className = 'connection-import-text';
    const title = document.createElement('strong');
    title.textContent = candidate.label;
    const detail = document.createElement('small');
    detail.textContent = `${candidate.user}@${candidate.host}:${candidate.port}/${candidate.database}`;
    text.append(title, detail);
    const badges = document.createElement('span');
    badges.className = 'connection-import-badges';
    const tags = [t(`connectionImport.from.${candidate.source}`)];
    if (candidate.ssl) tags.push(t(candidate.ssl_verification_downgraded ? 'connectionImport.tlsUnverified' : 'connectionImport.tls'));
    tags.push(t(candidate.has_password ? 'connectionImport.hasPassword' : 'connectionImport.noPassword'));
    if (candidate.already_saved) tags.push(t('connectionImport.alreadySaved'));
    if (result) tags.push(t(`connectionImport.status.${result.status}`));
    for (const tag of tags) {
      const badge = document.createElement('span');
      badge.className = 'badge';
      badge.textContent = tag;
      badges.append(badge);
    }
    if (result) badges.lastChild.classList.add(result.status === 'saved' ? 'success' : 'error');
    row.append(check, text, badges);
    list.append(row);
  }
  byId('connection-import-confirm').disabled = !candidates.length;
}

async function runConnectionImport() {
  const keys = [...byId('connection-import-list').querySelectorAll('input[type="checkbox"]:checked:not(:disabled)')].map((input) => input.value);
  const status = byId('connection-import-status');
  if (!keys.length) {
    status.textContent = t('connectionImport.nothingSelected');
    status.className = 'form-status error';
    return;
  }
  const confirm = byId('connection-import-confirm');
  confirm.disabled = true;
  status.textContent = t('connectionImport.testing', { count: keys.length });
  status.className = 'form-status';
  try {
    const results = await invoke('import_connections', { keys });
    const saved = results.filter((result) => result.status === 'saved').length;
    renderConnectionImport(new Map(results.map((result) => [result.key, result])));
    status.textContent = t('connectionImport.done', { count: saved, total: results.length });
    status.className = `form-status ${saved === results.length ? 'success' : 'error'}`;
    if (saved) await refreshConnections();
  } catch (error) {
    status.textContent = errorMessage(error, t);
    status.className = 'form-status error';
  } finally {
    confirm.disabled = false;
  }
}

async function saveCurrentConnection(event) {
  event.preventDefault();
  const request = connectionRequest();
  if (!(await testCurrentConnection())) return;
  setStatus(t('connectionForm.saving'));
  try {
    await invoke('save_connection', { request });
    hideForm();
    await refreshConnections();
  } catch (error) {
    setStatus(t('connectionForm.saveFailed'), 'error');
  }
}

function renderExplorerConnections() {
  const pane = byId('explorer-connections');
  pane.replaceChildren();
  for (const connection of state.connections) {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = `tree-item ${state.selectedConnectionId === connection.id ? 'selected' : ''}`;
    const name = document.createElement('span');
    name.textContent = connection.label;
    item.append(name, ...connectionBadges(connection));
    item.addEventListener('click', () => openExplorer(connection.id));
    pane.append(item);
  }
  if (!state.connections.length) {
    const empty = document.createElement('div');
    empty.className = 'empty-state';
    empty.textContent = t('explorer.noConnections');
    pane.append(empty);
  }
}

function renderQueryConnections() {
  const select = byId('query-connection');
  const selected = select.value;
  select.replaceChildren();
  const placeholder = document.createElement('option');
  placeholder.value = '';
  placeholder.textContent = t('common.selectConnected');
  select.append(placeholder);
  for (const connection of state.connections.filter((item) => item.state === 'connected')) {
    const option = document.createElement('option');
    option.value = connection.id;
    option.textContent = connectionOptionLabel(connection);
    select.append(option);
  }
  select.value = state.connections.some((item) => item.id === selected && item.state === 'connected') ? selected : '';
  syncConnectionMarkers();
}

function renderAdvancedConnections() {
  for (const id of ['dashboard-connection', 'admin-connection', 'backup-connection', 'assistant-connection', 'programming-connection']) {
    const select = byId(id);
    const selected = select.value;
    select.replaceChildren();
    const placeholder = document.createElement('option'); placeholder.value = ''; placeholder.textContent = t('common.selectConnected'); select.append(placeholder);
    for (const connection of state.connections.filter((item) => item.state === 'connected')) {
      const option = document.createElement('option'); option.value = connection.id; option.textContent = connectionOptionLabel(connection); select.append(option);
    }
    select.value = state.connections.some((item) => item.id === selected && item.state === 'connected') ? selected : '';
  }
  syncConnectionMarkers();
}

function scrollAssistantToBottom() {
  window.requestAnimationFrame(() => {
    const transcript = byId('assistant-history');
    transcript.scrollTop = transcript.scrollHeight;
  });
}

function renderAssistantHistory(history) {
  const transcript = byId('assistant-history'); transcript.replaceChildren();
  if (!history.length) { transcript.append(errorState(t('assistant.askAboutAQuerySchema'), t('assistant.theAssistantCanInspectThe'))); scrollAssistantToBottom(); return; }
  for (const message of history) {
    const bubble = document.createElement('article'); bubble.className = `assistant-message ${message.role === 'user' ? 'user' : ''}`;
    const label = document.createElement('small'); label.textContent = message.tool_label ? t('assistant.tool', { tool: message.tool_label }) : message.role === 'user' ? t('assistant.you') : t('nav.assistant');
    const content = document.createElement('div'); content.textContent = message.tool_failed ? t('assistant.toolFailed') : message.content; bubble.append(label, content); transcript.append(bubble);
  }
  scrollAssistantToBottom();
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
    if (requestEpoch === assistantRequestEpoch) { renderAssistantHistory([]); byId('assistant-status').textContent = t('assistant.couldNotLoadAssistantHistory'); }
  }
}

async function sendAssistant() {
  const id = byId('assistant-connection').value; const message = byId('assistant-message').value;
  if (!id) { byId('assistant-status').textContent = t('common.selectConnected'); return; }
  if (!message.trim()) return;
  const requestEpoch = ++assistantRequestEpoch;
  byId('send-assistant').disabled = true; byId('assistant-status').textContent = t('assistant.thinking');
  try {
    const reply = await invoke('assistant_send', { id, message });
    if (requestEpoch === assistantRequestEpoch) { renderAssistantHistory(reply.history); byId('assistant-message').value = ''; byId('assistant-status').textContent = t('assistant.tokens', { input: reply.input_tokens, output: reply.output_tokens }); }
  } catch (error) { if (requestEpoch === assistantRequestEpoch) byId('assistant-status').textContent = errorMessage(error, t, 'assistant.assistantRequestFailedConfigureA'); }
  finally { if (requestEpoch === assistantRequestEpoch) byId('send-assistant').disabled = false; }
}

function setAiReviewFocus(focus) {
  aiReviewFocus = AI_QUERY_REVIEW_FOCUSES.includes(focus) ? focus : 'general';
  for (const button of document.querySelectorAll('[data-ai-review-focus]')) {
    const active = button.dataset.aiReviewFocus === aiReviewFocus;
    button.classList.toggle('active', active);
    button.setAttribute('aria-pressed', String(active));
  }
}

function closeAiReviewDialog({ restoreFocus = true } = {}) {
  byId('ai-review-dialog').hidden = true;
  byId('ai-review-query-preview').textContent = '';
  byId('ai-review-note').value = '';
  aiReviewRequest = null;
  if (restoreFocus) aiReviewReturnFocus?.focus();
  aiReviewReturnFocus = null;
}

/** Shared by the SQL editor's and the Programming editor's "Review with AI" entry points. */
function openAiReviewDialogFor(connectionId, sql, title, returnView) {
  aiReviewRequest = { connectionId, sql, returnView };
  aiReviewReturnFocus = document.activeElement;
  setAiReviewFocus('general');
  byId('ai-review-title').textContent = title;
  byId('ai-review-query-preview').textContent = sql;
  byId('ai-review-note').value = '';
  byId('ai-review-dialog').hidden = false;
  window.setTimeout(() => document.querySelector('[data-ai-review-focus="general"]')?.focus(), 0);
}

function openAiReviewDialog() {
  const connectionId = byId('query-connection').value;
  const sql = selectedQueryText().trim();
  if (!connectionId) { byId('query-status').textContent = t('common.selectConnected'); return; }
  if (!sql) { byId('query-status').textContent = t('assistant.writeAQueryBeforeReviewing'); return; }
  saveCurrentQueryTab();
  openAiReviewDialogFor(connectionId, sql, t('assistant.reviewQueryWithAi'), 'query');
}

function openProgrammingAiReviewDialog() {
  const target = programmingEditorTarget;
  if (!target?.id) { setProgrammingEditorStatus(t('common.selectConnected'), 'error'); return; }
  let ddl;
  try { ddl = currentProgrammingDdl().trim(); }
  catch (error) { setProgrammingEditorStatus(errorMessage(error, t, 'assistant.completeTheRequiredFieldsBefore'), 'error'); return; }
  if (!ddl) { setProgrammingEditorStatus(t('assistant.writeADefinitionBeforeReviewing'), 'error'); return; }
  openAiReviewDialogFor(target.id, ddl, t('assistant.reviewObject', { kind: t(`kind.${target.kind}`) }), 'programming');
}

function goToAssistant(returnView) {
  assistantReturnView = returnView;
  const back = byId('assistant-back');
  back.hidden = !returnView;
  back.textContent = ASSISTANT_BACK_LABELS[returnView] || t('assistant.back');
  switchView('assistant');
}

async function submitAiReview() {
  if (!aiReviewRequest) return;
  const { connectionId, sql, returnView } = aiReviewRequest;
  const message = buildAiQueryReviewMessage(aiReviewFocus, sql, byId('ai-review-note').value);
  closeAiReviewDialog({ restoreFocus: false });
  byId('assistant-connection').value = connectionId; syncConnectionMarkers();
  byId('assistant-message').value = message;
  goToAssistant(returnView);
  await sendAssistant();
}

async function askAssistantAboutQueryStat(id, queryStat) {
  byId('assistant-connection').value = id; syncConnectionMarkers();
  byId('assistant-message').value = `${t('assistant.analyzeQueryStat', { calls: queryStat.calls, mean: queryStat.mean_exec_ms.toFixed(1), total: queryStat.total_exec_ms.toFixed(1), rows: queryStat.rows })}\n\n\`\`\`sql\n${queryStat.query}\n\`\`\``;
  goToAssistant('admin');
  await sendAssistant();
}

// Tells the backend that a long-running operation ended. Only the kind, outcome and duration are
// sent; the backend decides (10 s threshold, window focus, preference) and owns the text, so no
// SQL, result or file name can reach a system notification.
function reportOperationFinished(operation, outcome, startedAt) {
  if (!invoke) return;
  const durationMs = Math.max(0, Math.round(performance.now() - startedAt));
  void invoke('operation_finished', { operation, outcome, durationMs, locale: t.locale }).catch(() => {});
}

function operationId() { return `ui-${Date.now()}-${Math.random().toString(36).slice(2)}`; }

async function runBackup(restore = false) {
  const id = byId('backup-connection').value; if (!id) { byId('backup-status').textContent = t('common.selectConnected'); return; }
  if (restore && !await showConfirm(t('backup.restoreWillWriteDataInto'), t('backup.confirmRestore'), true, t('window.restore'))) return;
  const operation = operationId(); state.currentOperationId = operation; byId('cancel-operation').hidden = false; byId('backup-status').textContent = restore ? t('backup.restoring') : t('backup.creatingBackup'); byId('backup-log').textContent = '';
  for (const control of ['run-backup', 'run-restore', 'choose-backup-output', 'choose-restore-input', 'backup-connection', 'backup-output', 'backup-format', 'restore-input', 'restore-database']) byId(control).disabled = true;
  const options = restore ? { input: value('restore-input'), target_database: value('restore-database'), clean: false, single_transaction: true } : { output: value('backup-output'), format: byId('backup-format').value, compression: null, schemas: [], tables: [] };
  const startedAt = performance.now();
  try {
    const result = await invoke(restore ? 'run_restore' : 'run_backup', { id, operationId: operation, options });
    byId('backup-log').textContent = result.logs.join('\n'); byId('backup-status').textContent = result.cancelled ? t('query.cancelled') : result.succeeded ? t('query.completed') : t('backup.failedExit', { code: result.exit_code ?? '?' });
    reportOperationFinished(restore ? 'restore' : 'backup', result.cancelled ? 'cancelled' : result.succeeded ? 'succeeded' : 'failed', startedAt);
  } catch (error) {
    byId('backup-status').textContent = t('backup.backupOperationFailedCheckThe');
    if (error?.code !== 'write_cancelled') reportOperationFinished(restore ? 'restore' : 'backup', 'failed', startedAt);
  }
  finally {
    state.currentOperationId = null; byId('cancel-operation').hidden = true;
    for (const control of ['run-backup', 'run-restore', 'choose-backup-output', 'choose-restore-input', 'backup-connection', 'backup-output', 'backup-format', 'restore-input', 'restore-database']) byId(control).disabled = false;
  }
}

async function chooseBackupOutput() {
  try { const path = await invoke('choose_backup_output', { format: byId('backup-format').value }); if (path) byId('backup-output').value = path; }
  catch (error) { byId('backup-status').textContent = errorMessage(error, t, 'error.file_picker_unavailable'); }
}

async function chooseRestoreInput() {
  try { const path = await invoke('choose_restore_input'); if (path) byId('restore-input').value = path; }
  catch (error) { byId('backup-status').textContent = errorMessage(error, t, 'error.file_picker_unavailable'); }
}

async function cancelOperation() { if (!state.currentOperationId) return; byId('backup-status').textContent = t('query.cancelling'); try { await invoke('cancel_operation', { operationId: state.currentOperationId }); } catch (error) {} }

function metric(label, value) {
  const card = document.createElement('div'); card.className = 'metric-card';
  const caption = document.createElement('span'); caption.className = 'eyebrow'; caption.textContent = label;
  const number = document.createElement('strong'); number.textContent = value ?? '—';
  card.append(caption, number); return card;
}

async function loadDashboard(id) {
  const content = byId('dashboard-content'); content.replaceChildren();
  if (!id) { const empty = document.createElement('div'); empty.className = 'empty-state'; empty.textContent = t('admin.chooseAConnectedConnection'); content.append(empty); return; }
  const loading = document.createElement('div'); loading.className = 'empty-state'; loading.textContent = t('dashboard.loadingDashboard'); content.append(loading);
  try {
    const payload = await invoke('dashboard', { id });
    const data = payload.dashboard; const stats = payload.stats;
    content.replaceChildren();
    const metrics = document.createElement('div'); metrics.className = 'metric-grid';
    for (const [label, key] of [['PostgreSQL', 'pg_version'], [t('dashboard.databaseSize'), 'db_size'], [t('dashboard.cacheHit'), 'cache_hit'], [t('dashboard.activeConnections'), 'active_conn'], [t('dashboard.transactions'), 'commits'], [t('dashboard.rollbacks'), 'rollbacks']]) metrics.append(metric(label, data[key]));
    content.append(metrics);
    const info = document.createElement('div'); info.className = 'advanced-grid';
    info.append(dataPanel(t('connectionForm.database'), [[t('tableEditor.name'), data.db_name], [t('dashboard.encoding'), data.encoding], [t('dashboard.collation'), data.collation], [t('dashboard.uptime'), data.uptime]]));
    info.append(dataPanel(t('dashboard.largestTables'), (data.top_tables || []).map((row) => [`${row.schema}.${row.table}`, row.total_size])));
    info.append(dataPanel(t('dashboard.statistics'), [[t('dashboard.size'), stats.db?.size], [t('dashboard.cacheHit'), stats.db?.cache_hit_pct], [t('dashboard.deadlocks'), data.deadlocks], [t('dashboard.tempFiles'), data.temp_files]]));
    content.append(info);
  } catch (error) { content.replaceChildren(errorState(t('dashboard.dashboardUnavailable'), t('dashboard.checkPermissionsAndReconnect'))); }
}

function dataPanel(title, rows) {
  const panel = document.createElement('div'); panel.className = 'data-panel'; const heading = document.createElement('h3'); heading.textContent = title; panel.append(heading);
  for (const [label, value] of rows.slice(0, 12)) { const row = document.createElement('div'); row.className = 'data-row'; const key = document.createElement('span'); key.textContent = label; const val = document.createElement('strong'); val.textContent = value ?? '—'; row.append(key, val); panel.append(row); }
  return panel;
}

function indexPanel(id, schema, table, indexes) {
  const panel = document.createElement('section'); panel.className = 'data-panel';
  const heading = document.createElement('h3'); heading.textContent = t('table.indexes'); panel.append(heading);
  if (!indexes.length) {
    const empty = document.createElement('div'); empty.className = 'empty-state compact'; empty.textContent = t('table.noIndexes'); panel.append(empty); return panel;
  }
  for (const index of indexes.slice(0, 12)) {
    const row = document.createElement('div'); row.className = 'data-row object-definition-row';
    const name = document.createElement('span'); name.textContent = index.name;
    const definition = document.createElement('strong'); definition.textContent = index.definition || '—'; definition.title = index.definition || '';
    const edit = document.createElement('button'); edit.className = 'button small'; edit.type = 'button'; edit.textContent = t('common.edit');
    if (isReadOnly(id)) {
      edit.disabled = true;
      edit.title = t('environment.readOnlyHint');
    } else if (index.constraint_name) {
      edit.disabled = true;
      edit.title = t('table.managedByConstraint', { name: index.constraint_name });
    } else {
      edit.title = t('table.editNamed', { name: index.name });
      edit.addEventListener('click', () => void editIndexDefinition(id, schema, table, index.name));
    }
    row.append(name, definition, edit); panel.append(row);
  }
  return panel;
}

function codePanel(title, text) {
  const panel = document.createElement('section'); panel.className = 'data-panel code-panel';
  const heading = document.createElement('h3'); heading.textContent = title;
  const code = document.createElement('pre'); code.className = 'ddl-code'; code.textContent = text || '—';
  panel.append(heading, code);
  return panel;
}

function labeledControl(labelText, control) {
  const label = document.createElement('label'); label.append(document.createTextNode(labelText), control); return label;
}

function textInput(value = '', placeholder = '') {
  const input = document.createElement('input'); input.value = value ?? ''; input.placeholder = placeholder; input.autocomplete = 'off'; return input;
}

function selectInput(options, value) {
  const select = document.createElement('select');
  for (const optionValue of options) { const option = document.createElement('option'); option.value = optionValue; option.textContent = optionValue || '—'; select.append(option); }
  select.value = value ?? options[0]; return select;
}

function objectDialog(title) {
  const returnFocus = document.activeElement;
  const root = document.createElement('section'); root.className = 'object-dialog'; root.setAttribute('role', 'dialog'); root.setAttribute('aria-modal', 'true');
  const backdrop = document.createElement('div'); backdrop.className = 'object-dialog-backdrop';
  const panel = document.createElement('div'); panel.className = 'object-dialog-panel';
  const heading = document.createElement('div'); heading.className = 'object-dialog-heading';
  const name = document.createElement('h2'); name.textContent = title;
  const closeButton = document.createElement('button'); closeButton.className = 'button small'; closeButton.type = 'button'; closeButton.textContent = t('window.close');
  const body = document.createElement('div'); body.className = 'object-dialog-body';
  const actions = document.createElement('div'); actions.className = 'object-dialog-actions';
  const status = document.createElement('div'); status.className = 'form-status'; status.setAttribute('role', 'status');
  actions.append(status); heading.append(name, closeButton); panel.append(heading, body, actions); root.append(backdrop, panel); document.body.append(root);
  const close = () => { document.removeEventListener('keydown', onKeydown); root.remove(); if (returnFocus?.isConnected) returnFocus.focus({ preventScroll: true }); };
  const onKeydown = (event) => { if (event.key === 'Escape') close(); };
  closeButton.addEventListener('click', close); backdrop.addEventListener('click', close); document.addEventListener('keydown', onKeydown);
  window.setTimeout(() => body.querySelector('input, textarea, select, button')?.focus(), 0);
  return { root, body, actions, status, close };
}

const TABLE_COLUMN_TYPES = ['text', 'varchar(255)', 'integer', 'bigint', 'smallint', 'serial', 'bigserial', 'boolean', 'numeric(10,2)', 'real', 'double precision', 'date', 'timestamp with time zone', 'uuid', 'jsonb'];
byId('pg-type-suggestions').append(...TABLE_COLUMN_TYPES.map((type) => new Option(type, type)));

function checkboxControl(labelText, checked = false) {
  const label = document.createElement('label'); label.className = 'check-row';
  const input = document.createElement('input'); input.type = 'checkbox'; input.checked = checked;
  label.append(input, document.createTextNode(labelText)); return { label, input };
}

function columnEditorRow(initial = {}, mode = 'create') {
  const row = document.createElement('div'); row.className = 'column-editor-row'; row.dataset.originalName = initial.original_name ?? initial.name ?? '';
  const name = textInput(initial.name || '', 'column_name');
  const type = selectInput(TABLE_COLUMN_TYPES, TABLE_COLUMN_TYPES.includes(initial.data_type || initial.full_type) ? (initial.data_type || initial.full_type) : 'text');
  if (initial.full_type && !TABLE_COLUMN_TYPES.includes(initial.full_type)) { const option = document.createElement('option'); option.value = initial.full_type; option.textContent = initial.full_type; type.prepend(option); type.value = initial.full_type; }
  const nullable = checkboxControl(t('tableEditor.null'), initial.nullable ?? initial.is_nullable ?? true);
  const primary = checkboxControl('PK', initial.primary_key ?? initial.is_primary_key ?? false);
  const unique = checkboxControl(t('tableEditor.unique'), initial.unique ?? false);
  const defaultValue = textInput(initial.default ?? initial.column_default ?? '', 'default expression');
  const reference = textInput('', 'schema.table.column');
  const onDelete = selectInput(['', 'NO ACTION', 'RESTRICT', 'CASCADE', 'SET NULL', 'SET DEFAULT'], '');
  const remove = document.createElement('button'); remove.className = 'button small danger'; remove.type = 'button'; remove.textContent = mode === 'alter' ? t('tableEditor.remove') : t('common.delete');
  if (mode === 'alter') { unique.label.hidden = true; reference.parentElement?.remove(); }
  row.append(labeledControl(t('tableEditor.name'), name), labeledControl(t('tableEditor.type'), type), nullable.label, primary.label);
  if (mode === 'create') row.append(unique.label);
  row.append(labeledControl(t('tableEditor.default'), defaultValue));
  if (mode === 'create') row.append(labeledControl(t('tableEditor.references'), reference), labeledControl(t('tableEditor.onDelete'), onDelete));
  row.append(remove);
  remove.addEventListener('click', () => {
    if (mode === 'create' || !row.dataset.originalName) { row.remove(); return; }
    const removed = row.dataset.removed !== 'true'; row.dataset.removed = String(removed); row.classList.toggle('removed', removed); remove.textContent = removed ? t('tableEditor.undo') : t('tableEditor.remove');
    for (const control of row.querySelectorAll('input, select')) control.disabled = removed;
    remove.disabled = false;
  });
  return { row, name, type, nullable: nullable.input, primary: primary.input, unique: unique.input, defaultValue, reference, onDelete };
}

function readCreateColumn(editor) {
  const referenceText = editor.reference.value.trim();
  let references = null;
  if (referenceText) {
    const parts = referenceText.split('.');
    if (parts.length !== 3 || parts.some((part) => !part.trim())) throw new Error(t('tableEditor.referencesMustUseSchemaTable'));
    references = { schema: parts[0].trim(), table: parts[1].trim(), column: parts[2].trim(), on_delete: editor.onDelete.value || null };
  }
  return { name: editor.name.value.trim(), data_type: editor.type.value, nullable: editor.nullable.checked, primary_key: editor.primary.checked, unique: editor.unique.checked, default: editor.defaultValue.value.trim() || null, references };
}

function quotePreviewIdentifier(value) { return `"${String(value).replaceAll('"', '""')}"`; }

function createTablePreview(input) {
  const definitions = input.columns.map((column) => {
    let sql = `${quotePreviewIdentifier(column.name)} ${column.data_type}`;
    if (!column.nullable) sql += ' NOT NULL';
    if (column.unique) sql += ' UNIQUE';
    if (column.default) sql += ` DEFAULT ${column.default}`;
    if (column.references) sql += ` REFERENCES ${quotePreviewIdentifier(column.references.schema)}.${quotePreviewIdentifier(column.references.table)}(${quotePreviewIdentifier(column.references.column)})${column.references.on_delete ? ` ON DELETE ${column.references.on_delete}` : ''}`;
    return sql;
  });
  const primary = input.columns.filter((column) => column.primary_key).map((column) => quotePreviewIdentifier(column.name));
  if (primary.length) definitions.push(`PRIMARY KEY (${primary.join(', ')})`);
  return `CREATE TABLE ${quotePreviewIdentifier(input.schema)}.${quotePreviewIdentifier(input.table)} (\n  ${definitions.join(',\n  ')}\n);`;
}

function openCreateTableDialog(id, schema) {
  const dialog = objectDialog(t('explorer.newTable'));
  const basics = document.createElement('div'); basics.className = 'object-form-grid';
  const schemaInput = textInput(schema || 'public', 'schema'); const tableInput = textInput('', 'table_name');
  basics.append(labeledControl(t('tableEditor.schema'), schemaInput), labeledControl(t('trigger.table'), tableInput));
  const columns = document.createElement('div'); columns.className = 'column-editor-list';
  const editors = [];
  const addColumn = (initial = {}) => { const editor = columnEditorRow(initial, 'create'); editors.push(editor); columns.append(editor.row); return editor; };
  addColumn({ name: 'id', data_type: 'bigint', nullable: false, primary_key: true }); addColumn({ name: 'name', data_type: 'text', nullable: true });
  const add = document.createElement('button'); add.className = 'button small'; add.type = 'button'; add.textContent = t('tableEditor.addColumn'); add.addEventListener('click', () => addColumn());
  const preview = document.createElement('pre'); preview.className = 'object-sql-preview';
  const readInput = () => {
    const activeEditors = editors.filter((editor) => editor.row.isConnected);
    const input = { schema: schemaInput.value.trim(), table: tableInput.value.trim(), columns: activeEditors.map(readCreateColumn) };
    if (!input.schema || !input.table || !input.columns.length || input.columns.some((column) => !column.name)) throw new Error(t('tableEditor.schemaTableAndEveryColumn'));
    return input;
  };
  const refreshPreview = () => { try { preview.textContent = createTablePreview(readInput()); dialog.status.textContent = ''; } catch (error) { preview.textContent = `-- ${t('tableEditor.previewCreateHint')}`; dialog.status.textContent = error.message; } };
  dialog.body.append(basics, columns, add, preview); dialog.body.addEventListener('input', refreshPreview); dialog.body.addEventListener('change', refreshPreview); refreshPreview();
  const create = document.createElement('button'); create.className = 'button primary'; create.type = 'button'; create.textContent = t('tableEditor.createTable');
  create.addEventListener('click', async () => {
    let input; try { input = readInput(); } catch (error) { dialog.status.textContent = error.message; dialog.status.className = 'form-status error'; return; }
    create.disabled = true; dialog.status.textContent = t('tableEditor.creatingTable'); dialog.status.className = 'form-status';
    try { await invoke('create_table', { id, input }); dialog.close(); await openExplorer(id); openTable(id, input.schema, input.table); }
    catch (error) { dialog.status.textContent = errorMessage(error, t, 'tableEditor.couldNotCreateTable'); dialog.status.className = 'form-status error'; create.disabled = false; }
  });
  dialog.actions.append(create);
}

function alterTableInput(table, editors) {
  const columns = editors.filter((editor) => editor.row.isConnected).map((editor) => ({
    original_name: editor.row.dataset.originalName || null,
    name: editor.name.value.trim(), data_type: editor.type.value, nullable: editor.nullable.checked, primary_key: editor.primary.checked,
    default: editor.defaultValue.value.trim() || null, removed: editor.row.dataset.removed === 'true',
  }));
  if (!table.trim() || columns.some((column) => !column.removed && !column.name)) throw new Error(t('tableEditor.tableAndActiveColumnNames'));
  return { new_table_name: table.trim(), columns };
}

function openAlterTableDialog(id, schema, table, detail, options = {}) {
  const dialog = objectDialog(t('tableEditor.editTitle', { schema, table }));
  const tableName = textInput(table); dialog.body.append(labeledControl(t('tableEditor.tableName'), tableName));
  const list = document.createElement('div'); list.className = 'column-editor-list'; const editors = [];
  const addColumn = (initial = {}) => { const editor = columnEditorRow(initial, 'alter'); editors.push(editor); list.append(editor.row); return editor; };
  for (const column of detail.columns || []) addColumn({ ...column, original_name: column.name });
  const add = document.createElement('button'); add.className = 'button small'; add.type = 'button'; add.textContent = t('tableEditor.addColumn'); add.addEventListener('click', () => addColumn());
  const preview = document.createElement('pre'); preview.className = 'object-sql-preview'; preview.textContent = `-- ${t('tableEditor.previewAlterHint')}`;
  dialog.body.append(list, add, preview);
  const previewButton = document.createElement('button'); previewButton.className = 'button'; previewButton.type = 'button'; previewButton.textContent = t('tableEditor.refreshPreview');
  const loadPreview = async () => {
    const input = alterTableInput(tableName.value, editors); const result = await invoke('preview_alter_table', { id, schema, table, input }); preview.textContent = result.sql; return { input, result };
  };
  previewButton.addEventListener('click', async () => { previewButton.disabled = true; dialog.status.textContent = t('tableEditor.buildingPreview'); try { await loadPreview(); dialog.status.textContent = t('tableEditor.previewUpdated'); dialog.status.className = 'form-status success'; } catch (error) { dialog.status.textContent = errorMessage(error, t, 'tableEditor.couldNotBuildPreview'); dialog.status.className = 'form-status error'; } finally { previewButton.disabled = false; } });
  const apply = document.createElement('button'); apply.className = 'button primary'; apply.type = 'button'; apply.textContent = t('tableEditor.applyChanges');
  apply.addEventListener('click', async () => {
    apply.disabled = true;
    try {
      const { input, result } = await loadPreview();
      if (!result.statements.length) { await showAlert('There are no structural changes to apply.', t('tableEditor.tableUnchanged')); return; }
      const confirmed = await showConfirm(`${result.destructive ? t('tableEditor.theseChangesCanRemoveData') : t('tableEditor.applyThisStructuralChangeAtomically')}\n\n${result.sql}`, t('tableEditor.applyTableChanges'), result.destructive, t('tableEditor.apply'));
      if (!confirmed) return;
      dialog.status.textContent = t('tableEditor.applyingTableChanges'); dialog.status.className = 'form-status';
      await invoke('alter_table', { id, schema, table, input });
      dialog.close();
      if (options.preserveExplorer) {
        options.onApplied?.(input.new_table_name);
        filterExplorerTree();
      } else {
        await openTable(id, schema, input.new_table_name);
      }
    } catch (error) { dialog.status.textContent = errorMessage(error, t, 'tableEditor.couldNotAlterTable'); dialog.status.className = 'form-status error'; }
    finally { apply.disabled = false; }
  });
  dialog.actions.append(previewButton, apply);
}

async function createSchemaFromExplorer() {
  const id = state.selectedConnectionId; if (!id) return;
  const schema = await showPrompt(t('explorer.newSchemaMessage'), t('explorer.newSchema'), t('explorer.schemaName'), 'app'); if (!schema) return;
  try { await invoke('create_schema', { id, schema }); await openExplorer(id); }
  catch (error) { await showAlert(errorMessage(error, t, 'explorer.createSchemaFailed'), t('explorer.schemaNotCreated')); }
}

async function createSequenceFromExplorer() {
  const id = state.selectedConnectionId; const schema = state.selectedSchema; if (!id || !schema) return;
  const name = await showPrompt(t('explorer.newSequenceMessage', { schema }), t('explorer.newSequence'), t('explorer.sequenceName'), 'items_id_seq'); if (!name) return;
  try { await invoke('create_sequence', { id, schema, name }); await openExplorer(id); }
  catch (error) { await showAlert(errorMessage(error, t, 'explorer.createSequenceFailed'), t('explorer.sequenceNotCreated')); }
}

function openTriggerCreator(id, schema, onCreated = null) {
  const dialog = objectDialog(t('explorer.newTriggerTitle', { schema })); const grid = document.createElement('div'); grid.className = 'object-form-grid';
  const name = textInput('', 'audit_change'); const table = textInput('', 'table'); const timing = selectInput(['BEFORE', 'AFTER', 'INSTEAD OF'], 'BEFORE'); const events = textInput('INSERT', 'INSERT UPDATE'); const func = textInput('', 'schema.function_name');
  grid.append(labeledControl(t('trigger.name'), name), labeledControl(t('trigger.table'), table), labeledControl(t('trigger.timing'), timing), labeledControl(t('trigger.events'), events), labeledControl(t('trigger.function'), func)); dialog.body.append(grid);
  const create = document.createElement('button'); create.className = 'button primary'; create.type = 'button'; create.textContent = t('trigger.create');
  create.addEventListener('click', async () => { create.disabled = true; dialog.status.textContent = t('trigger.creating'); try { await invoke('create_trigger', { id, schema, input: { name: name.value.trim(), table: table.value.trim(), timing: timing.value, events: events.value.trim(), function: func.value.trim() } }); dialog.close(); if (onCreated) await onCreated(); else await openExplorer(id); } catch (error) { dialog.status.textContent = errorMessage(error, t, 'trigger.createFailed'); dialog.status.className = 'form-status error'; create.disabled = false; } });
  dialog.actions.append(create);
}

function triggerTemplate(schema) {
  return `CREATE OR REPLACE TRIGGER ${quotePreviewIdentifier('trigger_name')}\nBEFORE INSERT ON ${quotePreviewIdentifier(schema)}.${quotePreviewIdentifier('table_name')}\nFOR EACH ROW\nEXECUTE FUNCTION ${quotePreviewIdentifier(schema)}.${quotePreviewIdentifier('function_name')}();`;
}

function viewTemplate(schema) {
  return `CREATE OR REPLACE VIEW ${quotePreviewIdentifier(schema)}.${quotePreviewIdentifier('view_name')} AS\nSELECT\n  *\nFROM ${quotePreviewIdentifier(schema)}.${quotePreviewIdentifier('table_name')};`;
}

function definitionEditorDialog(id, title, ddl, kind, context = {}) {
  const definitions = {
    function: ['save_function_definition', t('definition.saveDefinition')],
    trigger: ['save_trigger_definition', t('definition.saveTrigger')],
    view: ['save_view_definition', t('definition.saveView')],
    sequence: ['save_sequence_definition', t('definition.saveSequence')],
    index: ['save_index_definition', t('definition.recreateIndex')],
  };
  const [saveCommand, saveLabel] = definitions[kind];
  const { onSaved, ...commandContext } = context;
  const dialog = objectDialog(title); const editor = document.createElement('textarea'); editor.className = 'definition-editor'; editor.value = ddl; editor.spellcheck = false; dialog.body.append(editor);
  if (kind === 'function') {
    const validate = document.createElement('button'); validate.className = 'button'; validate.type = 'button'; validate.textContent = t('definition.validate');
    validate.addEventListener('click', async () => { validate.disabled = true; dialog.status.textContent = t('definition.validatingInARolledBack'); try { const error = await invoke('validate_function_definition', { id, ddl: editor.value }); dialog.status.textContent = error || t('definition.definitionIsValid'); dialog.status.className = `form-status ${error ? 'error' : 'success'}`; } catch (error) { dialog.status.textContent = errorMessage(error, t, 'definition.validationFailed'); dialog.status.className = 'form-status error'; } finally { validate.disabled = false; } });
    dialog.actions.append(validate);
  }
  const save = document.createElement('button'); save.className = 'button primary'; save.type = 'button'; save.textContent = saveLabel;
  save.addEventListener('click', async () => {
    if (kind === 'index' && !await showConfirm(t('definition.dracoWillDropAndRecreate'), t('definition.recreateIndex'), true, t('definition.recreate'))) return;
    save.disabled = true; dialog.status.textContent = t('definition.savingDefinition');
    try {
      await invoke(saveCommand, { id, ...commandContext, ddl: editor.value });
      dialog.close();
      if (onSaved) await onSaved(); else await openExplorer(id);
    } catch (error) {
      dialog.status.textContent = errorMessage(error, t, 'definition.couldNotSaveDefinition'); dialog.status.className = 'form-status error'; save.disabled = false;
    }
  });
  dialog.actions.append(save);
}

async function openProgrammingFromExplorer(id, schema, object) {
  state.selectedConnectionId = id;
  state.selectedSchema = schema;
  byId('programming-connection').value = id; syncConnectionMarkers();
  switchView('programming');
  await loadProgrammingSchemas(id);
  byId('programming-schema').value = schema;
  await loadProgrammingObjects(id, schema);
  await openProgrammingObject(id, schema, object);
}

async function editFunctionDefinition(id, schema, name, kind = 'function') {
  await openProgrammingFromExplorer(id, schema, { name, kind, detail: kind });
}

async function newProgrammingFromExplorer(kind) {
  const id = state.selectedConnectionId;
  const schema = state.selectedSchema;
  if (!id || !schema) return;
  state.selectedConnectionId = id;
  state.selectedSchema = schema;
  byId('programming-connection').value = id; syncConnectionMarkers();
  switchView('programming');
  await loadProgrammingSchemas(id);
  byId('programming-schema').value = schema;
  await newProgrammingDefinition(kind);
}

async function editViewDefinition(id, schema, name, onSaved = null) {
  try {
    const payload = await invoke('table_detail', { id, schema, table: name });
    definitionEditorDialog(id, t('definition.viewTitle', { schema, name }), payload.ddl, 'view', { schema, name, ...(onSaved ? { onSaved } : {}) });
  } catch (error) {
    await showAlert(errorMessage(error, t, 'definition.couldNotLoadTheView'), t('definition.viewUnavailable'));
  }
}

function editSequenceDefinition(id, schema, object) {
  definitionEditorDialog(id, t('sequence.title', { schema, name: object.name }), object.definition || '', 'sequence', { schema, name: object.name });
}

async function editIndexDefinition(id, schema, table, name) {
  try {
    const ddl = await invoke('index_definition', { id, schema, table, name });
    definitionEditorDialog(id, t('definition.indexTitle', { schema, name }), ddl, 'index', {
      schema,
      table,
      name,
      onSaved: () => openTable(id, schema, table),
    });
  } catch (error) {
    await showAlert(errorMessage(error, t, 'definition.couldNotLoadTheIndex'), t('definition.indexUnavailable'));
  }
}

let programmingRequest = 0;
let programmingEditorTarget = null;
// Session-backed file cache for the prototype. The native filesystem bridge can replace this
// map without changing the editor workflow once repository checkout paths are available.
const programmingFileStore = new Map();
let programmingWorkspacePath = null;

// New/opened functions and procedures are edited as structured fields (schema, name,
// parameters, returns, language) plus a body-only code editor instead of one free-text
// DDL blob. `sliceFunctionDdl` splits an existing definition into that shape and verifies
// its own work by reassembling it; when it can't confidently do that (unusual formatting,
// LANGUAGE C, etc.) `target.structured` stays false and the editor falls back to holding
// the full DDL exactly like views and triggers always have.
// plpgsql (the default language) requires a BEGIN...END block — a bare SQL statement is not
// a valid plpgsql body on its own. RETURN QUERY is the usual way to produce rows for a
// SETOF/TABLE(...) return type; a scalar return uses a plain RETURN instead.
const FUNCTION_TEMPLATE_BODY = '\nBEGIN\n  -- for SETOF/TABLE(...) returns: RETURN QUERY SELECT ...;\n  -- for a scalar/void return: RETURN ...; (or omit RETURN for void)\nEND;\n';
const SQL_FUNCTION_TEMPLATE_BODY = '\nSELECT CURRENT_DATE;\n';

function defaultFunctionHeader(kind, schema) {
  const isProcedure = kind === 'procedure';
  return { kind: isProcedure ? 'procedure' : 'function', schema: schema || null, name: '', params: [], returns: isProcedure ? null : 'void', language: 'plpgsql', extra: '', tag: null };
}

function refreshProgrammingDdlPreview() {
  const wrap = byId('programming-ddl-preview-wrap');
  const target = programmingEditorTarget;
  if (!target?.structured) { wrap.hidden = true; byId('programming-ddl-preview').textContent = ''; return; }
  wrap.hidden = false;
  try { byId('programming-ddl-preview').textContent = currentProgrammingDdl(); }
  catch (error) { byId('programming-ddl-preview').textContent = `-- ${errorMessage(error, t, 'programming.completeTheRequiredFieldsTo')}`; }
}

function notifyProgrammingFormChanged() {
  if (programmingEditorIsDirty()) setProgrammingEditorStatus(t('programming.unsavedChanges'));
  refreshProgrammingDdlPreview();
  syncProgrammingSourceState();
}

function functionParamRow(initial = {}) {
  const row = document.createElement('div'); row.className = 'function-param-row';
  const mode = selectInput(['', 'IN', 'OUT', 'INOUT', 'VARIADIC'], initial.mode || ''); mode.className = 'pf-param-mode';
  const name = textInput(initial.name || '', 'param_name'); name.className = 'pf-param-name';
  const type = textInput(initial.type || '', 'integer'); type.className = 'pf-param-type';
  const defaultValue = textInput(initial.default || '', 'default expression'); defaultValue.className = 'pf-param-default';
  const remove = document.createElement('button'); remove.className = 'button small danger'; remove.type = 'button'; remove.textContent = t('tableEditor.remove');
  remove.addEventListener('click', () => { row.remove(); notifyProgrammingFormChanged(); });
  row.append(labeledControl(t('programming.mode'), mode), labeledControl(t('tableEditor.name'), name), labeledControl(t('tableEditor.type'), type), labeledControl(t('tableEditor.default'), defaultValue), remove);
  return row;
}

// The Returns field is a kind selector (void/record/trigger need nothing else; a scalar
// type, SETOF <type> and TABLE(columns) need a complement) plus a "…" button that opens a
// small picker for that complement. `#pf-returns` (hidden) always holds the actual raw
// PostgreSQL RETURNS clause text — the kind select and detail button are just a friendlier
// way to edit it. classifyReturns/composeReturns convert between the two representations.
function classifyReturns(returns) {
  const raw = (returns || 'void').trim();
  const lower = raw.toLowerCase();
  if (!raw || lower === 'void') return { kind: 'void', detail: '' };
  if (lower === 'record') return { kind: 'record', detail: '' };
  if (lower === 'trigger') return { kind: 'trigger', detail: '' };
  const setofMatch = raw.match(/^SETOF\s+([\s\S]+)$/i);
  if (setofMatch) return { kind: 'setof', detail: setofMatch[1].trim() };
  const tableMatch = raw.match(/^TABLE\s*\(([\s\S]*)\)$/i);
  if (tableMatch) return { kind: 'table', detail: tableMatch[1].trim() };
  return { kind: 'scalar', detail: raw };
}

function composeReturns(kind, detail) {
  if (kind === 'scalar') return detail || 'integer';
  if (kind === 'setof') return `SETOF ${detail || 'text'}`;
  if (kind === 'table') return `TABLE(${detail})`;
  return kind;
}

function syncPfReturnsDetailButton() {
  const kind = byId('pf-returns-kind').value;
  const button = byId('pf-returns-detail');
  button.hidden = !['scalar', 'setof', 'table'].includes(kind);
  button.textContent = kind === 'table' ? t('programming.columns') : '…';
}

function setPfReturnsFromKind(kind, detail) {
  byId('pf-returns-kind').value = kind;
  byId('pf-returns').value = composeReturns(kind, detail);
  syncPfReturnsDetailButton();
}

/**
 * A "pick a PostgreSQL type" control: a select preloaded with the same common types offered
 * for table columns, plus an "Other…" option that reveals a free-text field for anything not
 * on the list (arrays, precision types, extension types, etc.). Shared by the scalar/SETOF
 * return-type picker and each row of the return-table column editor.
 */
// A free-text type field with native browser autocomplete (via <datalist>) instead of a rigid
// <select>: presets like "varchar(255)" or "numeric(10,2)" are offered as suggestions but stay
// fully editable, so the length/precision can be changed without an escape-hatch "Other…" step.
function typeTextControl(initial, placeholder = 'integer') {
  const input = textInput(initial || '', placeholder);
  input.setAttribute('list', 'pg-type-suggestions');
  input.setAttribute('autocapitalize', 'off');
  return { input, value: () => input.value.trim() };
}

function openReturnTypeDialog(initial, onApply) {
  const dialog = objectDialog(t('programming.returnType'));
  const type = typeTextControl(initial);
  dialog.body.append(labeledControl(t('tableEditor.type'), type.input));
  const apply = document.createElement('button'); apply.className = 'button primary'; apply.type = 'button'; apply.textContent = t('tableEditor.apply');
  apply.addEventListener('click', () => {
    const value = type.value();
    if (!value) { dialog.status.textContent = t('programming.enterAType'); return; }
    onApply(value);
    dialog.close();
  });
  dialog.actions.append(apply);
}

function returnColumnRow(initial = {}) {
  const row = document.createElement('div'); row.className = 'function-param-row return-column-row';
  const name = textInput(initial.name || '', 'column_name'); name.className = 'rc-name';
  const type = typeTextControl(initial.type || ''); type.input.classList.add('rc-type');
  const remove = document.createElement('button'); remove.className = 'button small danger'; remove.type = 'button'; remove.textContent = t('tableEditor.remove');
  remove.addEventListener('click', () => row.remove());
  row.append(labeledControl(t('programming.column'), name), labeledControl(t('tableEditor.type'), type.input), remove);
  return row;
}

function readReturnColumnRow(row) {
  return {
    mode: null,
    name: row.querySelector('.rc-name').value.trim() || null,
    type: row.querySelector('.rc-type').value.trim(),
    default: null,
  };
}

function openReturnTableDialog(initialRaw, onApply) {
  const dialog = objectDialog(t('programming.returnTableColumns'));
  const list = document.createElement('div'); list.className = 'function-param-list';
  const initialColumns = initialRaw ? parseFunctionParameters(initialRaw) : [];
  list.append(...(initialColumns.length ? initialColumns : [{ name: '', type: '' }]).map((column) => returnColumnRow(column)));
  const add = document.createElement('button'); add.className = 'button small'; add.type = 'button'; add.textContent = t('tableEditor.addColumn');
  add.addEventListener('click', () => list.append(returnColumnRow()));
  dialog.body.append(list, add);
  const apply = document.createElement('button'); apply.className = 'button primary'; apply.type = 'button'; apply.textContent = t('tableEditor.apply');
  apply.addEventListener('click', () => {
    const columns = [...list.children].map(readReturnColumnRow);
    if (!columns.length || columns.some((column) => !column.name || !column.type)) { dialog.status.textContent = t('programming.everyColumnNeedsAName'); return; }
    onApply(formatFunctionParameters(columns));
    dialog.close();
  });
  dialog.actions.append(apply);
}

function openPfReturnsDetailDialog() {
  const kind = byId('pf-returns-kind').value;
  const current = classifyReturns(value('pf-returns'));
  if (kind === 'table') {
    openReturnTableDialog(current.detail, (raw) => { setPfReturnsFromKind('table', raw); notifyProgrammingFormChanged(); });
  } else {
    openReturnTypeDialog(current.detail, (type) => { setPfReturnsFromKind(kind, type); notifyProgrammingFormChanged(); });
  }
}

function renderProgrammingFunctionForm(target) {
  const container = byId('programming-function-form');
  if (!target || !target.structured) { container.hidden = true; byId('pf-params').replaceChildren(); return; }
  container.hidden = false;
  const header = target.header || defaultFunctionHeader(target.kind, target.schema);

  const schemaSelect = byId('pf-schema');
  const schemaOptions = [...byId('programming-schema').options].map((option) => option.value).filter(Boolean);
  if (header.schema && !schemaOptions.includes(header.schema)) schemaOptions.push(header.schema);
  schemaSelect.replaceChildren(...schemaOptions.map((schema) => new Option(schema, schema)));
  schemaSelect.value = header.schema || byId('programming-schema').value || '';

  byId('pf-name').value = header.name || '';

  byId('pf-returns-field').hidden = header.kind === 'procedure';
  const returnsInfo = classifyReturns(header.returns);
  setPfReturnsFromKind(returnsInfo.kind, returnsInfo.detail);

  const languageSelect = byId('pf-language');
  const customInput = byId('pf-language-custom');
  const known = [...languageSelect.options].some((option) => option.value === header.language);
  languageSelect.value = known ? header.language : '__custom__';
  customInput.hidden = known;
  customInput.value = known ? '' : (header.language || '');

  byId('pf-extra').value = header.extra || '';

  byId('pf-params').replaceChildren(...header.params.map((param) => functionParamRow(param)));
}

function functionFormLanguageValue() {
  const select = byId('pf-language');
  return select.value === '__custom__' ? value('pf-language-custom') : select.value;
}

/** Reads the structured header fields back out of the DOM (the live source of truth while editing). */
function readFunctionFormHeader() {
  const schema = value('pf-schema');
  const name = value('pf-name');
  if (!schema || !name) throw new Error(t('programming.schemaAndNameAreRequired'));
  const params = [...byId('pf-params').children].map((row) => ({
    mode: row.querySelector('.pf-param-mode').value || null,
    name: row.querySelector('.pf-param-name').value.trim() || null,
    type: row.querySelector('.pf-param-type').value.trim(),
    default: row.querySelector('.pf-param-default').value.trim() || null,
  })).filter((param) => param.mode || param.name || param.type || param.default);
  if (params.some((param) => !param.type)) throw new Error(t('programming.everyParameterNeedsAType'));
  const kind = programmingEditorTarget?.kind === 'procedure' ? 'procedure' : 'function';
  return {
    kind,
    schema,
    name,
    params,
    returns: kind === 'function' ? (value('pf-returns') || 'void') : null,
    language: functionFormLanguageValue() || 'plpgsql',
    extra: value('pf-extra'),
    tag: programmingEditorTarget?.header?.tag || null,
  };
}

/** Populates the editor (and structured form, when applicable) from a full DDL string and returns the canonical assembled DDL to use as the dirty-tracking baseline. */
function applyProgrammingDdl(target, ddl, { isNew = false } = {}) {
  target.structured = false;
  target.header = null;
  let editorValue = ddl;
  let canonical = ddl;
  if (['function', 'procedure'].includes(target.kind)) {
    if (isNew) {
      target.structured = true;
      target.header = defaultFunctionHeader(target.kind, target.schema);
      editorValue = FUNCTION_TEMPLATE_BODY;
      canonical = assembleFunctionDdl(target.header, editorValue);
    } else {
      const sliced = sliceFunctionDdl(ddl, target.kind);
      if (sliced) {
        target.structured = true;
        target.header = sliced.header;
        editorValue = sliced.body;
        canonical = assembleFunctionDdl(sliced.header, sliced.body);
      }
    }
  }
  setProgrammingEditorValue(editorValue);
  renderProgrammingFunctionForm(target);
  return canonical;
}

/** The full DDL that would be saved/validated/committed right now — assembled from the structured form when active, or the raw editor content otherwise. */
function currentProgrammingDdl() {
  const target = programmingEditorTarget;
  if (!target) return '';
  if (target.structured) return assembleFunctionDdl(readFunctionFormHeader(), byId('programming-editor').value);
  return byId('programming-editor').value;
}

function programmingEditorIsDirty() {
  if (!programmingEditorTarget) return false;
  try { return currentProgrammingDdl() !== programmingEditorTarget.originalDdl; }
  catch { return true; }
}

let programmingLastAlert = '';
let programmingAlertReset = null;

function setProgrammingEditorStatus(message, tone = '', options = {}) {
  const status = byId('programming-editor-status');
  status.textContent = message;
  status.className = `form-status ${tone}`.trim();
  const isLoadMessage = tone === 'success' && /\bloaded\b|\bopened\b|\bselected\b|workspace selected|^New definition\b/i.test(message);
  const shouldAlert = options.alert !== false && (tone === 'error' || (tone === 'success' && !isLoadMessage));
  if (shouldAlert && message && message !== programmingLastAlert) {
    programmingLastAlert = message;
    if (programmingAlertReset) window.clearTimeout(programmingAlertReset);
    programmingAlertReset = window.setTimeout(() => { programmingLastAlert = ''; }, 1200);
    window.setTimeout(() => { void showAlert(message, tone === 'error' ? t('programming.programmingError') : t('programming.programmingSuccess')); }, 0);
  }
}

function clearProgrammingEditor(message = t('programming.selectAnObjectToStart')) {
  programmingEditorTarget = null;
  const editor = byId('programming-editor');
  editor.disabled = true;
  setProgrammingEditorValue('');
  renderProgrammingFunctionForm(null);
  refreshProgrammingDdlPreview();
  byId('programming-editor-kind').textContent = t('programming.codeEditor');
  byId('programming-editor-title').textContent = t('programming.selectAnObject');
  byId('programming-reload').disabled = true;
  byId('programming-validate').disabled = true;
  byId('programming-review-ai').disabled = true;
  byId('programming-save').disabled = true;
  byId('programming-save-file').disabled = true;
  byId('programming-run').disabled = true;
  setProgrammingEditorStatus(message);
  renderProgrammingGithubBranches();
  syncProgrammingSourceState();
}

function setProgrammingEditorTarget(target, ddl) {
  programmingEditorTarget = { ...target };
  const canonical = applyProgrammingDdl(programmingEditorTarget, ddl, { isNew: Boolean(target.isNew) });
  programmingEditorTarget.originalDdl = canonical;
  programmingEditorTarget.deployedDdl = canonical;
  const editor = byId('programming-editor');
  editor.disabled = false;
  byId('programming-editor-kind').textContent = target.kind.toUpperCase();
  byId('programming-editor-title').textContent = target.title;
  byId('programming-reload').disabled = false;
  byId('programming-validate').disabled = !['function', 'procedure'].includes(target.kind);
  byId('programming-review-ai').disabled = false;
  byId('programming-save').disabled = false;
  byId('programming-save-file').disabled = false;
  byId('programming-run').disabled = !['function', 'procedure'].includes(target.kind);
  const structured = programmingEditorTarget.structured;
  const statusMessage = target.isNew
    ? (structured ? t('programming.newDefinitionFillInThe') : t('programming.newDefinitionEditAndSave'))
    : (structured ? t('programming.definitionLoadedSchemaNameParameters') : t('programming.definitionLoaded'));
  setProgrammingEditorStatus(statusMessage, 'success', { alert: Boolean(target.isNew) });
  byId('programming-browser-screen').hidden = true;
  byId('programming-editor-screen').hidden = false;
  document.querySelector('.programming-panel').classList.add('editor-open');
  byId('programming-github-diff').hidden = true;
  refreshProgrammingDdlPreview();
  syncProgrammingSourceState(target.isNew ? t('programming.newFileNotSaved') : undefined);
  void loadProgrammingGithub();
  editor.focus();
}

async function returnToProgrammingBrowser() {
  if (!await confirmProgrammingEditorReplacement()) return;
  byId('programming-editor-screen').hidden = true;
  byId('programming-browser-screen').hidden = false;
  document.querySelector('.programming-panel').classList.remove('editor-open');
  clearProgrammingEditor();
}

async function confirmProgrammingEditorReplacement() {
  if (!programmingEditorIsDirty()) return true;
  return showConfirm(t('programming.thisDefinitionHasUnsavedChanges'), t('programming.discardChanges'), false, t('programming.discard'));
}

async function selectRoutineDefinition(id, schema, object, preferredArgs = null) {
  const definitions = await invoke('function_definitions', { id, schema, name: object.name });
  if (!definitions.length) throw new Error(t('programming.definitionNotFound'));
  if (preferredArgs !== null) {
    const preferred = definitions.find((item) => item.args === preferredArgs);
    if (preferred) return preferred;
  }
  return definitions[0];
}

async function openProgrammingObject(id, schema, object, options = {}) {
  if (!options.skipDiscardCheck && !await confirmProgrammingEditorReplacement()) return;
  setProgrammingEditorStatus(t('programming.loadingNamed', { name: object.name }));
  try {
    let ddl = object.definition || '';
    let args = options.preferredArgs ?? object.identity_arguments ?? null;
    if (object.kind === 'view') {
      const payload = await invoke('table_detail', { id, schema, table: object.name });
      ddl = payload.ddl;
    } else if (['function', 'procedure'].includes(object.kind)) {
      const selected = await selectRoutineDefinition(id, schema, object, args);
      if (!selected) { setProgrammingEditorStatus(t('programming.objectSelectionCancelled')); return; }
      ddl = selected.ddl;
      args = selected.args;
    } else if (object.kind === 'trigger') {
      ddl = ddl.replace(/^CREATE\s+TRIGGER/i, 'CREATE OR REPLACE TRIGGER');
    }
    if (!ddl.trim()) throw new Error(t('programming.definitionNotFound'));
    const targetPath = programmingGithubPath({ schema, name: object.name, kind: object.kind, args });
    ddl = programmingFileStore.get(`draco-programming-file:${targetPath}`) || ddl;
    const signature = args === null ? '' : `(${args})`;
    setProgrammingEditorTarget({
      id,
      schema,
      name: object.name,
      kind: object.kind,
      title: `${schema}.${object.name}${signature}`,
      source: object,
      args,
      isNew: false,
    }, ddl);
  } catch (error) {
    setProgrammingEditorStatus(errorMessage(error, t, 'programming.couldNotLoadTheDefinition'), 'error');
  }
}

async function reloadProgrammingDefinition() {
  const target = programmingEditorTarget;
  if (!target || !await confirmProgrammingEditorReplacement()) return;
  if (target.isNew) {
    const canonical = applyProgrammingDdl(target, target.originalDdl, { isNew: true });
    target.originalDdl = canonical;
    target.deployedDdl = canonical;
    refreshProgrammingDdlPreview();
    setProgrammingEditorStatus(t('programming.templateRestored'), 'success');
    return;
  }
  await openProgrammingObject(target.id, target.schema, target.source, { skipDiscardCheck: true, preferredArgs: target.args });
}

async function validateProgrammingDefinition() {
  const target = programmingEditorTarget;
  if (!target || !['function', 'procedure'].includes(target.kind)) return;
  const button = byId('programming-validate');
  let ddl;
  try { ddl = currentProgrammingDdl(); }
  catch (error) { setProgrammingEditorStatus(errorMessage(error, t, 'programmingGit.completeTheRequiredFieldsFirst'), 'error'); return; }
  button.disabled = true;
  setProgrammingEditorStatus(t('definition.validatingInARolledBack'));
  try {
    const error = await invoke('validate_function_definition', { id: target.id, ddl });
    setProgrammingEditorStatus(error || t('programming.definitionIsValid'), error ? 'error' : 'success');
  } catch (error) {
    setProgrammingEditorStatus(errorMessage(error, t, 'definition.validationFailed'), 'error');
  } finally {
    button.disabled = false;
  }
}

async function saveProgrammingDefinition() {
  const target = programmingEditorTarget;
  if (!target) return;
  const button = byId('programming-save');
  let ddl;
  try { ddl = currentProgrammingDdl(); }
  catch (error) { setProgrammingEditorStatus(errorMessage(error, t, 'programming.completeTheRequiredFieldsBefore'), 'error'); return; }
  button.disabled = true;
  setProgrammingEditorStatus(t('definition.savingDefinition'));
  try {
    if (target.kind === 'file') {
      await saveProgrammingFile();
      button.disabled = false;
      return;
    }
    if (target.kind === 'view') await invoke('save_view_definition', { id: target.id, schema: target.schema, name: target.name, ddl });
    else if (target.kind === 'trigger') await invoke('save_trigger_definition', { id: target.id, ddl });
    else await invoke('save_function_definition', { id: target.id, ddl });
    target.originalDdl = ddl;
    target.deployedDdl = ddl;
    target.fileSavedDdl = ddl;
    target.isNew = false;
    if (target.structured) {
      const header = readFunctionFormHeader();
      target.schema = header.schema;
      target.name = header.name;
      target.title = `${header.schema}.${header.name}`;
      byId('programming-editor-title').textContent = target.title;
      // The saved definition may live in a different schema than the one picked in the
      // toolbar (the Schema field in the form is independently editable) — keep the toolbar
      // in sync so loadProgrammingObjects' own "did the user switch schema mid-flight" guard
      // (which compares against byId('programming-schema').value) doesn't discard this refresh
      // and leave the new/moved definition invisible until the dropdown is touched by hand.
      const schemaSelect = byId('programming-schema');
      if (schemaSelect.value !== target.schema && [...schemaSelect.options].some((option) => option.value === target.schema)) {
        schemaSelect.value = target.schema;
      }
    }
    setProgrammingEditorStatus(t('programming.definitionSaved'), 'success');
    syncProgrammingSourceState(t('programming.compiledSavedToPostgresql'));
    await loadProgrammingObjects(target.id, target.schema);
    const kindLabel = { view: t('programming.view'), trigger: t('programming.trigger'), function: t('trigger.function'), procedure: t('programming.procedure') }[target.kind] || t('programming.definition');
    await showAlert(t('programming.savedKind', { kind: kindLabel }), t('programming.saved'));
  } catch (error) {
    setProgrammingEditorStatus(errorMessage(error, t, 'programming.couldNotSaveTheDefinition'), 'error');
  } finally {
    button.disabled = false;
  }
}

function programmingLocalFileKey(target = programmingEditorTarget) {
  const path = programmingGithubPath(target);
  return path ? `draco-programming-file:${path}` : null;
}

async function chooseProgrammingWorkspace() {
  try {
    const folder = await invoke('choose_programming_workspace');
    if (folder) {
      programmingWorkspacePath = folder;
      void savePreferences({ programming_workspace: folder });
      byId('programming-source-state').textContent = t('programming.workspaceFolder', { folder });
      setProgrammingEditorStatus(t('programming.programmingWorkspaceSelected'), 'success');
      await loadProgrammingLocalFiles();
    }
  } catch (error) { setProgrammingEditorStatus(errorMessage(error, t, 'programming.couldNotChooseTheWorkspace'), 'error'); }
}

async function clearProgrammingWorkspace() {
  if (!programmingWorkspacePath || !await showConfirm(t('programming.removeTheProgrammingWorkspaceFrom'), t('programming.clearWorkspace'), false, t('history.clearAction'))) return;
  programmingWorkspacePath = null;
  await savePreferences({ programming_workspace: null });
  byId('programming-local-files-count').textContent = '0';
  byId('programming-local-files-list').replaceChildren(errorState(t('programming.noWorkspaceSelected'), t('programming.chooseAFolderToBrowse')));
  byId('programming-source-state').textContent = t('programming.noWorkspaceSelected');
  setProgrammingEditorStatus(t('programming.programmingWorkspaceCleared'), 'success');
}

async function loadProgrammingLocalFiles() {
  const list = byId('programming-local-files-list');
  const count = byId('programming-local-files-count');
  if (!programmingWorkspacePath) return;
  try {
    const files = await invoke('list_programming_files', { workspace: programmingWorkspacePath });
    count.textContent = String(files.length);
    list.replaceChildren();
    if (!files.length) { list.append(errorState(t('programming.noSqlFilesFound'), t('programming.saveADefinitionToThis'))); return; }
    for (const file of files) {
      const row = document.createElement('div'); row.className = 'programming-local-file';
      const label = document.createElement('span'); label.textContent = file;
      const open = document.createElement('button'); open.className = 'button small'; open.type = 'button'; open.innerHTML = '<span class="button-icon" aria-hidden="true">▣</span>Open';
      open.addEventListener('click', () => void openProgrammingLocalFile(file));
      row.append(label, open); list.append(row);
    }
  } catch (error) { count.textContent = '—'; list.replaceChildren(errorState(t('programming.workspaceUnavailable'), errorMessage(error, t, 'programming.couldNotReadSqlFiles'))); }
}

async function openProgrammingLocalFile(relativePath) {
  if (!programmingWorkspacePath || !await confirmProgrammingEditorReplacement()) return;
  try {
    const content = await invoke('read_programming_file', { workspace: programmingWorkspacePath, relativePath });
    const name = relativePath.split('/').pop().replace(/\.sql$/i, '');
    const schema = relativePath.split('/')[0] || byId('programming-schema').value || 'public';
    const target = { id: byId('programming-connection').value, schema, name, kind: 'file', title: relativePath, source: { name, kind: 'file' }, args: null, isNew: false, localPath: relativePath };
    setProgrammingEditorTarget(target, content);
    programmingEditorTarget.originalDdl = content;
    programmingEditorTarget.deployedDdl = content;
    byId('programming-validate').disabled = true;
    byId('programming-run').disabled = true;
    setProgrammingEditorStatus(t('programming.openedFromDisk', { path: relativePath }), 'success');
  } catch (error) { setProgrammingEditorStatus(errorMessage(error, t, 'programming.couldNotOpenTheLocal'), 'error'); }
}

async function saveProgrammingFile() {
  const target = programmingEditorTarget;
  if (!target) return;
  let ddl;
  try {
    if (target.structured) {
      const header = readFunctionFormHeader();
      target.header = header;
      target.schema = header.schema;
      target.name = header.name;
      target.title = `${header.schema}.${header.name}`;
      byId('programming-editor-title').textContent = target.title;
    }
    ddl = currentProgrammingDdl();
  } catch (error) { setProgrammingEditorStatus(errorMessage(error, t, 'programming.completeTheRequiredFieldsBefore'), 'error'); return; }
  const key = programmingLocalFileKey(target);
  if (!key) { setProgrammingEditorStatus(t('programming.saveTheDefinitionWithA'), 'error'); return; }
  try {
    if (!programmingWorkspacePath) await chooseProgrammingWorkspace();
    if (programmingWorkspacePath) {
      const savedPath = await invoke('save_programming_file', { workspace: programmingWorkspacePath, relativePath: programmingGithubPath(target), content: ddl });
      programmingFileStore.set(key, ddl);
      target.fileSavedDdl = ddl;
      setProgrammingEditorStatus(t('programming.fileSavedOnDisk', { path: savedPath || programmingGithubPath(target) }), 'success');
    } else {
      programmingFileStore.set(key, ddl);
      setProgrammingEditorStatus(t('programming.fileDraftSavedForThis'), 'success');
    }
    syncProgrammingSourceState(t('programming.savedFileNotCompiled'));
  } catch (error) {
    setProgrammingEditorStatus(errorMessage(error, t, 'programming.couldNotSaveTheLocal'), 'error');
  }
}

function openProgrammingRunDialog() {
  const target = programmingEditorTarget;
  if (!target || !['function', 'procedure'].includes(target.kind)) return;
  if (programmingEditorIsDirty()) {
    setProgrammingEditorStatus(t('programming.saveOrCompileTheCurrent'), 'error');
    return;
  }
  const params = byId('programming-run-params');
  params.replaceChildren();
  const header = target.header || { params: [] };
  for (const [index, param] of (header.params || []).entries()) {
    if (param.mode === 'OUT') continue;
    const label = document.createElement('label');
    const labelText = document.createElement('span'); labelText.textContent = `${param.name || t('programming.parameterNumber', { number: index + 1 })} · ${param.type}`; label.replaceChildren(labelText);
    const input = document.createElement('input');
    input.dataset.paramIndex = String(index);
    input.placeholder = param.default ? t('programming.paramDefault', { value: param.default }) : 'NULL';
    input.autocomplete = 'off';
    label.append(input); params.append(label);
  }
  if (!params.children.length) params.append(Object.assign(document.createElement('p'), { className: 'form-status', textContent: t('programming.thisRoutineHasNoInput') }));
  byId('programming-run-title').textContent = t('programming.runTitle', { kind: t(`kind.${target.kind}`), schema: target.schema, name: target.name });
  byId('programming-run-status').textContent = '';
  byId('programming-run-dialog').hidden = false;
  params.querySelector('input')?.focus();
}

function closeProgrammingRunDialog() { byId('programming-run-dialog').hidden = true; }

function sqlLiteral(raw) {
  const text = raw.trim();
  if (!text || /^null$/i.test(text)) return 'NULL';
  if (/^(true|false)$/i.test(text) || /^-?(?:\d+(?:\.\d*)?|\.\d+)$/.test(text)) return text;
  return `'${text.replaceAll("'", "''")}'`;
}

async function runProgrammingDefinition() {
  const target = programmingEditorTarget;
  if (!target || !['function', 'procedure'].includes(target.kind)) return;
  const inputs = [...byId('programming-run-params').querySelectorAll('input')];
  const header = target.header || { params: [] };
  const args = inputs.map((input) => sqlLiteral(input.value));
  const qualified = `${target.schema}.${target.name}`;
  const sql = target.kind === 'procedure' ? `CALL ${qualified}(${args.join(', ')});` : `SELECT * FROM ${qualified}(${args.join(', ')});`;
  const output = byId('programming-execution-output');
  const resultView = byId('programming-execution-result');
  const summary = byId('programming-execution-summary');
  byId('programming-run-confirm').disabled = true;
  byId('programming-run-status').textContent = t('programming.executing');
  try {
    const result = await invoke('execute_query', { id: target.id, sql, operationId: operationId() });
    output.hidden = false;
    summary.textContent = t('results.summary', { count: result.rows?.length || 0, ms: result.duration_ms ?? 0 });
    const displayCell = (cell) => {
      if (cell == null) return 'NULL';
      if (typeof cell === 'object') return JSON.stringify(cell);
      return String(cell);
    };
    resultView.textContent = result.columns?.length
      ? [result.columns.join(' | '), ...(result.rows || []).map((row) => result.columns.map((column) => displayCell(row[column])).join(' | '))].join('\n')
      : t('programming.executionCompletedWithoutAResult');
    byId('programming-run-status').textContent = t('programming.completed');
    closeProgrammingRunDialog();
  } catch (error) {
    byId('programming-run-status').textContent = errorMessage(error, t, 'programming.executionFailed');
  } finally { byId('programming-run-confirm').disabled = false; }
}

async function newProgrammingDefinition(kind) {
  const id = byId('programming-connection').value;
  const schema = byId('programming-schema').value;
  if (!id || !schema || !await confirmProgrammingEditorReplacement()) return;
  const isTrigger = kind === 'trigger';
  setProgrammingEditorTarget({
    id,
    schema,
    name: null,
    kind,
    title: t('programming.newIn', { kind: t(`kind.${kind}`), schema }),
    source: null,
    args: null,
    isNew: true,
  }, isTrigger ? triggerTemplate(schema) : kind === 'view' ? viewTemplate(schema) : '');
}

function programmingGithubPath(target = programmingEditorTarget) {
  if (!target?.name) return null;
  if (target.localPath) return target.localPath;
  const folder = { view: 'views', function: 'functions', procedure: 'procedures', trigger: 'triggers' }[target.kind] || `${target.kind}s`;
  const safe = (part) => String(part).replace(/[^a-zA-Z0-9._-]+/g, '_').replace(/^_+|_+$/g, '') || 'object';
  const overload = target.args ? `__${safe(target.args)}` : '';
  return `${safe(target.schema)}/${folder}/${safe(target.name)}${overload}.sql`;
}

function renderProgrammingGithubBranches() {
  const branch = byId('programming-github-branch');
  const base = byId('programming-github-base');
  const previousBranch = branch.value;
  const previousBase = base.value;
  branch.replaceChildren(new Option(githubConnection?.connected ? t('programmingGit.chooseBranch') : t('programmingGit.connectGithubInPreferences'), ''));
  base.replaceChildren(new Option(t('programmingGit.baseBranch'), ''));
  for (const item of githubBranches) {
    branch.append(new Option(`${item.name}${item.protected ? t('programmingGit.protected') : ''}`, item.name));
    base.append(new Option(item.name, item.name));
  }
  const fallback = githubConnection?.default_branch || githubBranches[0]?.name || '';
  branch.value = githubBranches.some((item) => item.name === previousBranch) ? previousBranch : fallback;
  base.value = githubBranches.some((item) => item.name === previousBase) ? previousBase : fallback;
  const ready = Boolean(githubConnection?.connected && githubBranches.length && programmingGithubPath());
  branch.disabled = !ready;
  base.disabled = !ready;
  byId('programming-github-message').disabled = !ready;
  for (const id of ['programming-github-load', 'programming-github-diff-deployed', 'programming-github-diff-branches', 'programming-github-commit', 'programming-github-pr']) byId(id).disabled = !ready;
  const pendingForTarget = pendingGithubCommit && pendingGithubCommit.path === programmingGithubPath() && pendingGithubCommit.branch === branch.value;
  byId('programming-github-push').disabled = !ready || !pendingForTarget;
  byId('programming-github-status').textContent = githubConnection?.connected
    ? `${githubConnection.owner}/${githubConnection.repository}`
    : t('programmingGit.githubNotConnected');
  renderProgrammingRepositoryWorkspace();
}

// The repository is a workspace choice, not a credential setting.  The first implementation
// exposes the connected repository here and keeps the selector ready for the multi-repository
// provider API that will follow.
function renderProgrammingRepositoryWorkspace() {
  const repository = byId('programming-repository');
  const branch = byId('programming-workspace-branch');
  if (!repository || !branch) return;
  const repoValue = githubConnection?.connected ? `${githubConnection.owner}/${githubConnection.repository}` : '';
  repository.replaceChildren(new Option(repoValue ? t('programmingGit.chooseRepository') : t('programmingGit.connectGithubInPreferences'), ''));
  for (const item of githubRepositories) {
    const option = new Option(`${item.owner}/${item.name}${item.private ? t('programmingGit.private') : ''}`, `${item.owner}/${item.name}`);
    repository.append(option);
  }
  repository.value = repoValue;
  branch.replaceChildren(new Option(githubConnection?.connected ? t('programmingGit.chooseABranch') : t('programmingGit.connectGithubInPreferences'), ''));
  for (const item of githubBranches) branch.append(new Option(item.name, item.name));
  branch.value = programmingEditorTarget?.activeBranch || githubConnection?.default_branch || '';
  byId('programming-repository-label').textContent = repoValue || t('programmingGit.selectARepository');
}

function syncProgrammingSourceState(message) {
  const stateLabel = byId('programming-source-state');
  if (!stateLabel) return;
  const dirty = programmingEditorIsDirty();
  stateLabel.textContent = message || (dirty ? t('programming.modifiedNotCompiled') : programmingEditorTarget ? t('programming.savedFileCompiled') : t('programming.noFileSelected'));
  stateLabel.className = `source-state ${dirty ? 'dirty' : programmingEditorTarget ? 'compiled' : ''}`.trim();
}

async function loadProgrammingGithub() {
  try {
    if (!githubConnection) githubConnection = await invoke('github_status');
    if (!githubConnection.connected) {
      githubBranches = [];
      renderProgrammingGithubBranches();
      return;
    }
    githubRepositories = await invoke('github_repositories');
    githubBranches = githubConnection.owner && githubConnection.repository ? await invoke('github_branches') : [];
    renderProgrammingGithubBranches();
  } catch (error) {
    githubBranches = [];
    githubRepositories = [];
    renderProgrammingGithubBranches();
    byId('programming-github-status').textContent = errorMessage(error, t, 'programmingGit.githubUnavailable');
  }
}

function lineDiff(before, after, beforeLabel, afterLabel) {
  const left = before.replace(/\r\n/g, '\n').split('\n');
  const right = after.replace(/\r\n/g, '\n').split('\n');
  let prefix = 0;
  while (prefix < left.length && prefix < right.length && left[prefix] === right[prefix]) prefix += 1;
  let suffix = 0;
  while (suffix < left.length - prefix && suffix < right.length - prefix && left[left.length - 1 - suffix] === right[right.length - 1 - suffix]) suffix += 1;
  const contextStart = Math.max(0, prefix - 3);
  const leftEnd = Math.min(left.length, left.length - suffix + 3);
  const rightEnd = Math.min(right.length, right.length - suffix + 3);
  const output = [`--- ${beforeLabel}`, `+++ ${afterLabel}`, `@@ -${contextStart + 1},${leftEnd - contextStart} +${contextStart + 1},${rightEnd - contextStart} @@`];
  for (let index = contextStart; index < prefix; index += 1) output.push(` ${left[index]}`);
  for (let index = prefix; index < left.length - suffix; index += 1) output.push(`-${left[index]}`);
  for (let index = prefix; index < right.length - suffix; index += 1) output.push(`+${right[index]}`);
  for (let index = Math.max(prefix, right.length - suffix); index < rightEnd; index += 1) output.push(` ${right[index]}`);
  if (before === after) output.push(t('programmingGit.noDifferences'));
  return output.join('\n');
}

function showProgrammingGithubDiff(diff) {
  const panel = byId('programming-github-diff');
  panel.textContent = diff || t('programmingGit.noDifferences2');
  panel.hidden = false;
  panel.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
}

async function loadProgrammingGithubFile() {
  const target = programmingEditorTarget;
  const branch = byId('programming-github-branch').value;
  const path = programmingGithubPath(target);
  if (!target || !branch || !path || !await confirmProgrammingEditorReplacement()) return;
  setProgrammingEditorStatus(t('programmingGit.loadingFrom', { path, branch }));
  try {
    const content = await invoke('github_file', { branch, path });
    if (content === null) throw new Error(t('programmingGit.notOnBranch', { branch }));
    const canonical = applyProgrammingDdl(target, content, { isNew: false });
    target.originalDdl = canonical;
    target.activeBranch = branch;
    refreshProgrammingDdlPreview();
    setProgrammingEditorStatus(t('programmingGit.loadedFrom', { branch }), 'success');
  } catch (error) {
    setProgrammingEditorStatus(errorMessage(error, t, 'programmingGit.couldNotLoadTheGithub'), 'error');
  }
}

async function diffProgrammingDeployed() {
  const target = programmingEditorTarget;
  const branch = byId('programming-github-branch').value;
  const path = programmingGithubPath(target);
  if (!target || !branch || !path) return;
  setProgrammingEditorStatus(t('programmingGit.comparingDeployed', { branch }));
  try {
    const content = await invoke('github_file', { branch, path });
    showProgrammingGithubDiff(lineDiff(target.deployedDdl, content || '', 'deployed database', `github/${branch}/${path}`));
    setProgrammingEditorStatus(t('programmingGit.comparedDeployed', { branch }), 'success');
  } catch (error) {
    setProgrammingEditorStatus(errorMessage(error, t, 'programmingGit.couldNotCompareTheDeployed'), 'error');
  }
}

async function diffProgrammingBranches() {
  const head = byId('programming-github-branch').value;
  const base = byId('programming-github-base').value;
  if (!head || !base) return;
  setProgrammingEditorStatus(t('programmingGit.comparingBranches', { base, head }));
  try {
    showProgrammingGithubDiff(await invoke('github_compare', { base, head }));
    setProgrammingEditorStatus(t('programmingGit.comparedBranches', { base, head }), 'success');
  } catch (error) {
    setProgrammingEditorStatus(errorMessage(error, t, 'programmingGit.couldNotCompareBranches'), 'error');
  }
}

async function commitProgrammingGithubFile() {
  const target = programmingEditorTarget;
  const branch = byId('programming-github-branch').value;
  const path = programmingGithubPath(target);
  const message = value('programming-github-message');
  if (!target || !branch || !path) return;
  if (!message) {
    setProgrammingEditorStatus(t('programmingGit.enterACommitMessageFirst'), 'error');
    byId('programming-github-message').focus();
    return;
  }
  let content;
  try { content = currentProgrammingDdl(); }
  catch (error) { setProgrammingEditorStatus(errorMessage(error, t, 'programmingGit.completeTheRequiredFieldsFirst'), 'error'); return; }
  pendingGithubCommit = { branch, path, content, message };
  target.activeBranch = branch;
  byId('programming-github-message').value = '';
  setProgrammingEditorStatus(t('programmingGit.commitPrepared', { branch }), 'success');
  renderProgrammingGithubBranches();
}

async function pushProgrammingGithubFile() {
  if (!pendingGithubCommit) {
    setProgrammingEditorStatus(t('programmingGit.faAOCommitLocal'), 'error');
    return;
  }
  const { branch, path, content, message } = pendingGithubCommit;
  setProgrammingEditorStatus(t('programmingGit.pushing', { branch }));
  try {
    const url = await invoke('github_commit_file', { branch, path, content, message });
    if (programmingEditorTarget) programmingEditorTarget.originalDdl = content;
    pendingGithubCommit = null;
    setProgrammingEditorStatus(t('programmingGit.pushed', { branch, url }), 'success');
    renderProgrammingGithubBranches();
  } catch (error) { setProgrammingEditorStatus(errorMessage(error, t, 'programmingGit.couldNotPushTheCommit'), 'error'); }
}

function openProgrammingPullRequestForm() {
  const form = byId('programming-github-pr-form');
  form.hidden = false;
  const target = programmingEditorTarget;
  if (!byId('programming-github-pr-title').value) byId('programming-github-pr-title').value = target ? `Update ${target.schema}.${target.name}` : '';
  byId('programming-github-pr-title').focus();
}

async function createProgrammingPullRequest(event) {
  event.preventDefault();
  const head = byId('programming-github-branch').value;
  const base = byId('programming-github-base').value;
  const title = value('programming-github-pr-title');
  const body = value('programming-github-pr-body');
  setProgrammingEditorStatus(t('programmingGit.creatingPr', { head, base }));
  try {
    const pull = await invoke('github_create_pull_request', { title, body, head, base });
    byId('programming-github-pr-form').hidden = true;
    await navigator.clipboard.writeText(pull.html_url);
    setProgrammingEditorStatus(t('programmingGit.prCreated', { number: pull.number }), 'success');
  } catch (error) {
    setProgrammingEditorStatus(errorMessage(error, t, 'programmingGit.couldNotCreateThePull'), 'error');
  }
}

function programmingObjectKindLabel(kind) {
  return t(`kind.${kind === 'procedure' || kind === 'trigger' ? kind : 'function'}`);
}

/**
 * Drops a function, procedure or trigger after a type-to-confirm prompt (same pattern as
 * dropping an extension or a role). Objects PostgreSQL installed via `CREATE EXTENSION`
 * (`object.is_extension`, set from the `pg_depend` check the backend also re-checks before
 * the actual `DROP`) are refused up front with an explanation instead of a confirm dialog —
 * they can only go away with the extension itself.
 */
async function deleteSchemaProgrammingObject(id, schema, object, onDeleted) {
  const kindLabel = programmingObjectKindLabel(object.kind);
  if (object.is_extension) {
    await showAlert(t('programming.extensionOwned', { name: object.name }), t('programming.cannotDelete', { kind: kindLabel }));
    return;
  }
  const signature = object.kind === 'trigger' ? `on ${schema}.${object.parent_table}` : `(${object.identity_arguments || ''})`;
  const confirmation = await showDangerPrompt(t('programming.dropConfirm', { name: object.name, kind: kindLabel, signature }), t('programming.dropTitle', { kind: kindLabel }), t('programming.dropNameLabel', { kind: kindLabel }), object.name);
  if (confirmation !== object.name) { if (confirmation !== null) await showAlert(t('programming.dropNameMismatch', { kind: kindLabel }), t('programming.notDropped', { kind: kindLabel })); return; }
  try {
    if (object.kind === 'trigger') {
      await invoke('delete_trigger', { id, schema, table: object.parent_table, name: object.name });
    } else {
      await invoke('delete_routine', { id, schema, name: object.name, identityArguments: object.identity_arguments || '', isProcedure: object.kind === 'procedure' });
    }
    await onDeleted?.();
  } catch (error) {
    await showAlert(errorMessage(error, t, 'programming.dropFailed'), t('programming.notDropped', { kind: kindLabel }));
  }
}

function programmingGroup(title, objects, id, schema, onChanged) {
  const panel = document.createElement('section'); panel.className = 'data-panel programming-group';
  const heading = document.createElement('div'); heading.className = 'programming-group-heading';
  const label = document.createElement('h3'); label.textContent = title;
  const count = document.createElement('span'); count.className = 'badge'; count.textContent = String(objects.length);
  heading.append(label, count); panel.append(heading);
  if (!objects.length) {
    const empty = document.createElement('div'); empty.className = 'empty-state compact'; empty.textContent = t('programming.noneInSchema', { title: title.toLowerCase() }); panel.append(empty); return panel;
  }
  for (const object of objects) {
    const row = document.createElement('article'); row.className = 'programming-object';
    const detail = document.createElement('div');
    const name = document.createElement('strong'); name.textContent = object.name;
    const signature = document.createElement('small'); signature.textContent = object.detail || object.kind;
    detail.append(name, signature);
    const actions = document.createElement('div'); actions.className = 'programming-object-actions';
    const open = document.createElement('button'); open.className = 'button small'; open.type = 'button'; open.textContent = object.kind === 'trigger' ? t('trigger.table') : t('programming.open');
    open.addEventListener('click', () => object.kind === 'view' ? openTable(id, schema, object.name, 'view') : openSchemaObject(id, schema, object));
    const edit = document.createElement('button'); edit.className = 'button small'; edit.type = 'button'; edit.textContent = t('common.edit');
    edit.addEventListener('click', () => void openProgrammingObject(id, schema, object));
    actions.append(open, edit);
    if (['function', 'procedure', 'trigger'].includes(object.kind)) {
      const del = document.createElement('button'); del.className = 'button small danger'; del.type = 'button'; del.textContent = t('common.delete');
      del.disabled = Boolean(object.is_extension);
      del.title = object.is_extension ? t('explorer.extensionObject') : t(`explorer.deleteKind.${object.kind}`);
      del.addEventListener('click', () => void deleteSchemaProgrammingObject(id, schema, object, onChanged));
      actions.append(del);
    }
    row.append(detail, actions); panel.append(row);
  }
  return panel;
}

async function loadProgrammingObjects(id, schema) {
  const request = ++programmingRequest;
  const content = byId('programming-content');
  const status = byId('programming-status');
  if (!id || !schema) {
    content.replaceChildren(errorState(t('programming.chooseAConnectionAndSchema'), t('programming.viewsFunctionsProceduresAndTriggers')));
    status.textContent = '';
    return;
  }
  state.selectedConnectionId = id;
  state.selectedSchema = schema;
  const writable = !isReadOnly(id);
  byId('programming-new-function').disabled = !writable;
  byId('programming-new-procedure').disabled = !writable;
  byId('programming-new-trigger').disabled = !writable;
  byId('programming-new-view').disabled = !writable;
  content.replaceChildren(errorState(t('programming.loadingProgrammingObjects'), t('programming.readingViewsFunctionsProceduresAnd')));
  status.textContent = t('programming.loadingNamed', { name: schema });
  try {
    const [objects, tables] = await Promise.all([
      invoke('list_schema_objects', { id, schema }),
      invoke('list_tables', { id, schema }),
    ]);
    if (request !== programmingRequest || byId('programming-schema').value !== schema) return;
    const programming = groupSchemaObjects(objects).programming;
    const functions = programming.filter((object) => object.kind === 'function');
    const procedures = programming.filter((object) => object.kind === 'procedure');
    const triggers = programming.filter((object) => object.kind === 'trigger');
    const views = tables.filter((object) => object.kind === 'view').map((object) => ({ ...object, detail: formatEstimatedRows(object.estimated_rows) }));
    const refresh = () => loadProgrammingObjects(id, schema);
    content.replaceChildren(
      programmingGroup(t('programming.views'), views, id, schema, refresh),
      programmingGroup(t('programming.functions'), functions, id, schema, refresh),
      programmingGroup(t('programming.procedures'), procedures, id, schema, refresh),
      programmingGroup(t('programming.triggers'), triggers, id, schema, refresh),
    );
    const total = programming.length + views.length;
    status.textContent = t('programming.objectCount', { count: total, schema });
  } catch (error) {
    if (request !== programmingRequest) return;
    content.replaceChildren(errorState(t('programming.programmingObjectsUnavailable'), t('programming.checkSchemaPermissionsAndTry')));
    status.textContent = errorMessage(error, t, 'programming.couldNotLoadProgrammingObjects');
  }
}

async function loadProgrammingSchemas(id) {
  const request = ++programmingRequest;
  const schemaSelect = byId('programming-schema');
  const previous = schemaSelect.value;
  schemaSelect.replaceChildren(new Option(t('programming.selectASchema'), ''));
  schemaSelect.disabled = true;
  byId('programming-new-function').disabled = true;
  byId('programming-new-procedure').disabled = true;
  byId('programming-new-trigger').disabled = true;
  byId('programming-new-view').disabled = true;
  if (!id) {
    byId('programming-content').replaceChildren(errorState(t('admin.chooseAConnectedConnection'), t('programming.schemasAndProgrammingObjectsWill')));
    byId('programming-status').textContent = '';
    return;
  }
  byId('programming-content').replaceChildren(errorState(t('explorer.loadingSchemas'), t('programming.readingTheSelectedDatabase')));
  try {
    const schemas = await invoke('list_schemas', { id });
    if (request !== programmingRequest || byId('programming-connection').value !== id) return;
    for (const schema of schemas) schemaSelect.append(new Option(schema.name, schema.name));
    schemaSelect.disabled = false;
    const preferred = schemas.some((schema) => schema.name === previous) ? previous : schemas.some((schema) => schema.name === state.selectedSchema) ? state.selectedSchema : schemas[0]?.name || '';
    schemaSelect.value = preferred;
    await loadProgrammingObjects(id, preferred);
  } catch (error) {
    if (request !== programmingRequest) return;
    byId('programming-content').replaceChildren(errorState(t('programming.schemasUnavailable'), t('palette.reconnectAndTryAgain')));
    byId('programming-status').textContent = errorMessage(error, t, 'explorer.schemasFailed');
  }
}

function openProgramming() {
  const connection = byId('programming-connection');
  if (!connection.value && state.connections.some((item) => item.id === state.selectedConnectionId && item.state === 'connected')) connection.value = state.selectedConnectionId;
  void loadProgrammingSchemas(connection.value);
}

// CSV/JSON import into the open table. The file comes only from the native picker; the backend
// rereads the table, validates the mapping and copies every row in one transaction.
const tableImport = { id: null, schema: null, table: null, file: null, preview: null, operationId: null, onImported: null, returnFocus: null };

function tableImportSource() {
  return {
    path: tableImport.file.path,
    format: byId('table-import-format').value,
    delimiter: byId('table-import-delimiter').value,
    has_header: byId('table-import-header').checked,
    empty_as_null: byId('table-import-empty-null').checked,
  };
}

function setTableImportStatus(message, kind = '') {
  const status = byId('table-import-status');
  status.textContent = message;
  status.className = `form-status ${kind}`;
}

function openTableImportDialog(id, schema, table, onImported) {
  Object.assign(tableImport, { id, schema, table, file: null, preview: null, operationId: null, onImported, returnFocus: document.activeElement });
  byId('table-import-target').textContent = `${schema}.${table}`;
  byId('table-import-file').value = '';
  byId('table-import-preview').replaceChildren();
  byId('table-import-mapping').replaceChildren();
  syncTableImportControls();
  setTableImportStatus(t('tableImport.chooseFileHint'));
  byId('table-import-dialog').hidden = false;
  byId('table-import-choose').focus();
}

function closeTableImportDialog() {
  if (tableImport.operationId) return;
  byId('table-import-dialog').hidden = true;
  tableImport.returnFocus?.focus?.();
}

function syncTableImportControls() {
  const csv = byId('table-import-format').value === 'csv';
  for (const control of ['table-import-delimiter', 'table-import-header', 'table-import-empty-null']) byId(control).disabled = !csv || Boolean(tableImport.operationId);
  byId('table-import-preview-button').disabled = !tableImport.file || Boolean(tableImport.operationId);
  byId('table-import-run').disabled = !tableImport.preview || Boolean(tableImport.operationId);
  byId('table-import-run').hidden = Boolean(tableImport.operationId);
  byId('table-import-cancel-run').hidden = !tableImport.operationId;
  byId('table-import-choose').disabled = Boolean(tableImport.operationId);
  byId('table-import-close').disabled = Boolean(tableImport.operationId);
}

async function chooseTableImportFile() {
  try {
    const file = await invoke('choose_table_import_file');
    if (!file) return;
    tableImport.file = file;
    tableImport.preview = null;
    byId('table-import-file').value = file.file_name;
    byId('table-import-format').value = file.format;
    if (file.file_name.toLowerCase().endsWith('.tsv')) byId('table-import-delimiter').value = '\\t';
    byId('table-import-mapping').replaceChildren();
    byId('table-import-preview').replaceChildren();
    syncTableImportControls();
    await previewTableImport();
  } catch (error) {
    setTableImportStatus(errorMessage(error, t, 'error.file_picker_unavailable'), 'error');
  }
}

async function previewTableImport() {
  if (!tableImport.file) return;
  setTableImportStatus(t('tableImport.reading'));
  tableImport.preview = null;
  syncTableImportControls();
  try {
    const preview = await invoke('preview_table_import', { id: tableImport.id, schema: tableImport.schema, table: tableImport.table, source: tableImportSource() });
    tableImport.preview = preview;
    renderTableImportPreview(preview);
    setTableImportStatus(t('tableImport.previewReady', { count: preview.total_rows }), 'success');
  } catch (error) {
    byId('table-import-preview').replaceChildren();
    byId('table-import-mapping').replaceChildren();
    setTableImportStatus(errorMessage(error, t, 'tableImport.previewFailed'), 'error');
  } finally {
    syncTableImportControls();
  }
}

function renderTableImportPreview(preview) {
  const mapping = byId('table-import-mapping');
  mapping.replaceChildren();
  preview.source_columns.forEach((column, index) => {
    const row = document.createElement('label'); row.className = 'table-import-map-row';
    const name = document.createElement('code'); name.textContent = column;
    const arrow = document.createElement('span'); arrow.textContent = '→'; arrow.setAttribute('aria-hidden', 'true');
    const select = document.createElement('select'); select.dataset.sourceColumn = String(index);
    select.setAttribute('aria-label', t('tableImport.targetFor', { column }));
    const skip = document.createElement('option'); skip.value = ''; skip.textContent = t('tableImport.skipColumn'); select.append(skip);
    for (const target of preview.table_columns) {
      const option = document.createElement('option'); option.value = target.name;
      option.textContent = `${target.name} · ${target.data_type}${!target.nullable && !target.has_default ? ` · ${t('tableImport.required')}` : ''}`;
      select.append(option);
    }
    select.value = preview.suggested_mapping[index] || '';
    row.append(name, arrow, select); mapping.append(row);
  });

  const wrap = byId('table-import-preview'); wrap.replaceChildren();
  if (!preview.sample_rows.length) { wrap.append(errorState(t('tableImport.noRows'), t('tableImport.noRowsMessage'))); return; }
  const tableElement = document.createElement('table');
  const head = document.createElement('tr');
  for (const column of preview.source_columns) { const cell = document.createElement('th'); cell.textContent = column; head.append(cell); }
  const thead = document.createElement('thead'); thead.append(head);
  const tbody = document.createElement('tbody');
  for (const values of preview.sample_rows) {
    const row = document.createElement('tr');
    for (const value of values) {
      const cell = document.createElement('td');
      if (value === null) { cell.textContent = 'NULL'; cell.className = 'null-cell'; } else cell.textContent = value;
      row.append(cell);
    }
    tbody.append(row);
  }
  tableElement.append(thead, tbody);
  const caption = document.createElement('small'); caption.textContent = t('tableImport.sampleCaption', { shown: preview.sample_rows.length, count: preview.total_rows });
  wrap.append(caption, tableElement);
}

async function runTableImport() {
  const mapping = [...byId('table-import-mapping').querySelectorAll('select')]
    .filter((select) => select.value)
    .map((select) => ({ source_column: Number(select.dataset.sourceColumn), table_column: select.value }));
  if (!mapping.length) { setTableImportStatus(t('validation.importMappingEmpty'), 'error'); return; }
  const targets = mapping.map((entry) => entry.table_column);
  const duplicate = targets.find((name, index) => targets.indexOf(name) !== index);
  if (duplicate) { setTableImportStatus(t('validation.importMappingDuplicate', { name: duplicate }), 'error'); return; }
  const count = tableImport.preview.total_rows;
  if (!await showConfirm(t('tableImport.confirm', { count, table: `${tableImport.schema}.${tableImport.table}` }), t('tableImport.confirmTitle'), false, t('tableImport.run'))) return;
  tableImport.operationId = operationId();
  syncTableImportControls();
  setTableImportStatus(t('tableImport.importing', { count }));
  try {
    const result = await invoke('run_table_import', { id: tableImport.id, schema: tableImport.schema, table: tableImport.table, operationId: tableImport.operationId, input: { source: tableImportSource(), mapping } });
    if (result.cancelled) {
      setTableImportStatus(t('tableImport.cancelled'), 'error');
    } else {
      setTableImportStatus(t('tableImport.done', { count: result.rows_imported }), 'success');
      tableImport.onImported?.();
    }
  } catch (error) {
    setTableImportStatus(error?.code === 'write_cancelled' ? errorMessage(error, t) : `${errorMessage(error, t, 'tableImport.failed')} ${t('tableImport.rolledBack')}`, 'error');
  } finally {
    // The authorization is consumed by an import attempt; another import needs the file again.
    tableImport.operationId = null;
    tableImport.preview = null;
    tableImport.file = null;
    byId('table-import-file').value = '';
    syncTableImportControls();
  }
}

async function cancelTableImport() {
  if (!tableImport.operationId) return;
  setTableImportStatus(t('tableImport.cancelling'));
  try { await invoke('cancel_operation', { operationId: tableImport.operationId }); } catch { /* the import reports the final state */ }
}

function tableMaintenancePanel(id, schema, table, detail) {
  const panel = document.createElement('section'); panel.className = 'data-panel maintenance-panel';
  const heading = document.createElement('div'); heading.className = 'maintenance-heading';
  const title = document.createElement('h3'); title.textContent = t('table.tableMaintenance');
  const actions = document.createElement('div'); actions.className = 'maintenance-actions';
  const status = document.createElement('div'); status.className = 'form-status maintenance-status'; status.setAttribute('role', 'status');
  const operations = [['vacuum', t('table.vacuum')], ['analyze', t('table.analyze')], ['vacuum_analyze', t('table.vacuumAnalyze')], ['vacuum_full', t('table.vacuumFull')]];
  const editStructure = document.createElement('button'); editStructure.className = 'button small'; editStructure.type = 'button'; editStructure.textContent = t('table.editStructure');
  editStructure.addEventListener('click', () => openAlterTableDialog(id, schema, table, detail));
  actions.append(editStructure);
  for (const [operation, label] of operations) {
    const button = document.createElement('button'); button.className = `button small ${operation === 'vacuum_full' ? 'danger' : ''}`; button.type = 'button'; button.textContent = label;
    button.addEventListener('click', async () => {
      if (operation === 'vacuum_full') {
        const objectName = `${schema}.${table}`;
        const confirmation = await showDangerPrompt(t('table.vacuumFullConfirm', { name: objectName }), t('table.runVacuumFull'), t('table.schemaAndTable'), objectName);
        if (confirmation !== objectName) { if (confirmation !== null) await showAlert('The table name did not match. Maintenance was not started.', 'VACUUM FULL cancelled'); return; }
      }
      for (const control of actions.querySelectorAll('button')) control.disabled = true;
      status.textContent = t('table.runningOperation', { operation: label }); status.className = 'form-status maintenance-status';
      try {
        await invoke('run_table_maintenance', { id, schema, table, operation });
        status.textContent = t('table.operationCompleted', { operation: label });
        status.className = 'form-status maintenance-status success';
      } catch (error) {
        status.textContent = (error?.key ? errorMessage(error, t) : error?.message || t('table.operationFailed', { operation: label }));
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

function formatTableCell(valueJson) {
  if (valueJson === 'null') return 'NULL';
  if (typeof valueJson !== 'string') return '—';
  if (valueJson.startsWith('"')) {
    try { return JSON.parse(valueJson); } catch { return valueJson; }
  }
  return valueJson;
}

function tableDataPanel(id, schema, table, { importable = false } = {}) {
  const pageSize = 50;
  let offset = 0;
  let requestSequence = 0;
  let loading = false;
  const panel = document.createElement('section'); panel.className = 'data-panel table-data-panel';
  const heading = document.createElement('div'); heading.className = 'table-data-heading';
  const headingCopy = document.createElement('div');
  const title = document.createElement('h3'); title.textContent = t('table.tableData');
  const summary = document.createElement('small'); summary.textContent = t('table.loadingRows');
  headingCopy.append(title, summary);
  const controls = document.createElement('div'); controls.className = 'table-data-controls';
  const insert = document.createElement('button'); insert.className = 'button small'; insert.type = 'button'; insert.textContent = t('table.insertRow');
  // Read-only connections browse the data without row editing; the server would refuse it anyway.
  const readOnly = isReadOnly(id);
  insert.hidden = readOnly;
  const refresh = document.createElement('button'); refresh.className = 'button small'; refresh.type = 'button'; refresh.textContent = t('table.refresh');
  const previous = document.createElement('button'); previous.className = 'button small'; previous.type = 'button'; previous.textContent = t('table.previous');
  const next = document.createElement('button'); next.className = 'button small'; next.type = 'button'; next.textContent = t('explorer.sequenceNext');
  const importButton = document.createElement('button'); importButton.className = 'button small'; importButton.type = 'button'; importButton.textContent = t('tableImport.open');
  importButton.hidden = readOnly || !importable;
  importButton.addEventListener('click', () => openTableImportDialog(id, schema, table, () => loadPage()));
  controls.append(insert, importButton, refresh, previous, next); heading.append(headingCopy, controls);
  const status = document.createElement('div'); status.className = 'form-status table-data-status'; status.setAttribute('role', 'status');
  const grid = document.createElement('div'); grid.className = 'table-data-grid';
  panel.append(heading, status, grid);
  let canPrevious = false;
  let canNext = false;

  const setLoading = (value) => {
    loading = value;
    insert.disabled = value; refresh.disabled = value;
    previous.disabled = value || !canPrevious; next.disabled = value || !canNext;
  };
  const rowKeys = (row, primaryKeys) => primaryKeys.map((column) => ({ column, value_json: row[column] }));
  const parseJsonInput = async (value, context) => {
    try { return JSON.parse(value); }
    catch { await showAlert(t('table.jsonRequired', { context }), t('table.invalidJson')); return undefined; }
  };

  const loadPage = async () => {
    if (loading) return;
    const request = ++requestSequence;
    setLoading(true); status.textContent = t('table.loadingRows'); status.className = 'form-status table-data-status';
    try {
      const result = await invoke('browse_table_data', { id, schema, table, offset, limit: pageSize });
      if (request !== requestSequence || !panel.isConnected) return;
      const columns = result.columns || [];
      const rows = result.rows || [];
      const primaryKeys = result.primary_key_columns || [];
      const editable = primaryKeys.length > 0 && !readOnly;
      const first = result.total ? offset + 1 : 0;
      const last = Math.min(offset + rows.length, result.total);
      summary.textContent = `${t('table.pageRange', { first, last, total: result.total })}${primaryKeys.length ? ` · ${t('table.keyColumns', { columns: primaryKeys.join(', ') })}` : t('table.readOnlyWithoutAPrimary')}`;
      canPrevious = offset > 0;
      canNext = offset + rows.length < result.total;
      status.textContent = '';
      grid.replaceChildren();
      if (!rows.length) { grid.append(errorState(t('table.noRows'), t('table.insertARowOrRefresh'))); return; }
      const tableElement = document.createElement('table');
      const head = document.createElement('thead'); const header = document.createElement('tr');
      for (const column of columns) { const cell = document.createElement('th'); cell.scope = 'col'; cell.textContent = column; header.append(cell); }
      if (editable) { const actions = document.createElement('th'); actions.scope = 'col'; actions.textContent = t('table.actions'); header.append(actions); }
      head.append(header); tableElement.append(head);
      const body = document.createElement('tbody');
      for (const row of rows) {
        const tr = document.createElement('tr');
        for (const column of columns) {
          const td = document.createElement('td');
          if (editable) {
            td.className = 'editable-table-cell';
            const edit = document.createElement('button'); edit.className = 'table-cell-button'; edit.type = 'button'; edit.textContent = formatTableCell(row[column]); edit.title = t('table.editNamed', { name: column });
            edit.addEventListener('click', async () => {
              const valueJson = await showPrompt(t('table.cellValuePrompt', { column }), t('table.editTableCell'), column, '', row[column]);
              if (valueJson === null) return;
              if (await parseJsonInput(valueJson, t('table.theCellValue')) === undefined) return;
              setLoading(true); status.textContent = t('table.updatingColumn', { column });
              try {
                await invoke('update_table_cell', { id, schema, table, input: { keys: rowKeys(row, primaryKeys), column, value_json: valueJson } });
                status.textContent = t('table.columnUpdated', { column }); status.className = 'form-status table-data-status success';
                setLoading(false); await loadPage();
              } catch (error) {
                status.textContent = errorMessage(error, t, 'table.couldNotUpdateTheCell'); status.className = 'form-status table-data-status error'; setLoading(false);
              }
            });
            td.append(edit);
          } else {
            td.textContent = formatTableCell(row[column]);
          }
          tr.append(td);
        }
        if (editable) {
          const td = document.createElement('td'); td.className = 'table-row-actions';
          const remove = document.createElement('button'); remove.className = 'button small danger'; remove.type = 'button'; remove.textContent = t('common.delete');
          remove.addEventListener('click', async () => {
            const identity = primaryKeys.map((column) => `${column}=${formatTableCell(row[column])}`).join(', ');
            if (!await showConfirm(t('table.deleteRowConfirm', { identity }), t('table.deleteTableRow'), true)) return;
            setLoading(true); status.textContent = t('table.deletingRow');
            try {
              await invoke('delete_table_row', { id, schema, table, input: { keys: rowKeys(row, primaryKeys) } });
              if (rows.length === 1 && offset > 0) offset = Math.max(0, offset - pageSize);
              status.textContent = t('table.rowDeleted'); status.className = 'form-status table-data-status success'; setLoading(false); await loadPage();
            } catch (error) {
              status.textContent = errorMessage(error, t, 'table.couldNotDeleteTheRow'); status.className = 'form-status table-data-status error'; setLoading(false);
            }
          });
          td.append(remove); tr.append(td);
        }
        body.append(tr);
      }
      tableElement.append(body); grid.append(tableElement);
    } catch (error) {
      if (request !== requestSequence || !panel.isConnected) return;
      summary.textContent = t('table.rowsUnavailable');
      grid.replaceChildren(errorState(t('table.couldNotLoadTableData'), t('table.checkTablePermissionsAndRefresh')));
      status.textContent = errorMessage(error, t, 'table.couldNotLoadTableData'); status.className = 'form-status table-data-status error';
    } finally {
      if (request === requestSequence) setLoading(false);
    }
  };

  insert.addEventListener('click', async () => {
    const valuesJson = await showPrompt('Enter a JSON object with the columns to insert. Omitted columns keep their PostgreSQL defaults. Use {} for DEFAULT VALUES.', t('table.insertTableRow'), t('table.jsonRow'), '{"column":"value"}', '{}');
    if (valuesJson === null) return;
    const value = await parseJsonInput(valuesJson, t('table.theTableRow'));
    if (value === undefined) return;
    if (value === null || Array.isArray(value) || typeof value !== 'object') { await showAlert('A new table row must be a JSON object.', t('table.invalidTableRow')); return; }
    setLoading(true); status.textContent = t('table.insertingRow'); status.className = 'form-status table-data-status';
    try {
      await invoke('insert_table_row', { id, schema, table, input: { values_json: valuesJson } });
      status.textContent = t('table.rowInserted'); status.className = 'form-status table-data-status success'; setLoading(false); await loadPage();
    } catch (error) {
      status.textContent = errorMessage(error, t, 'table.couldNotInsertTheRow'); status.className = 'form-status table-data-status error'; setLoading(false);
    }
  });
  refresh.addEventListener('click', () => void loadPage());
  previous.addEventListener('click', () => { offset = Math.max(0, offset - pageSize); void loadPage(); });
  next.addEventListener('click', () => { offset += pageSize; void loadPage(); });
  window.setTimeout(() => void loadPage(), 0);
  return panel;
}

function foreignKeyMapPanel(id, rows) {
  const panel = document.createElement('section'); panel.className = 'data-panel fk-map-panel';
  const heading = document.createElement('h3'); heading.textContent = t('table.foreignKeys'); panel.append(heading);
  if (!rows.length) { panel.append(errorState(t('table.noForeignKeys'), t('table.noIncomingOrOutgoingRelationships'))); return panel; }
  for (const row of rows) {
    const button = document.createElement('button'); button.className = 'fk-map-row'; button.type = 'button';
    const copy = document.createElement('span');
    const name = document.createElement('strong'); name.textContent = row.constraint_name;
    const detail = document.createElement('small'); detail.textContent = `${row.direction} · ${row.foreign_schema}.${row.foreign_table}.${row.foreign_column}`;
    const arrow = document.createElement('span'); arrow.className = 'fk-map-arrow'; arrow.textContent = '→'; arrow.setAttribute('aria-hidden', 'true');
    copy.append(name, detail); button.append(copy, arrow);
    button.addEventListener('click', () => openTable(id, row.foreign_schema, row.foreign_table));
    panel.append(button);
  }
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
  attributes.push(role.connection_limit === -1 ? t('roles.unlimitedConnections') : t('connections.count', { count: role.connection_limit }));
  if (role.valid_until) attributes.push(t('roles.validUntilValue', { date: role.valid_until }));
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
  const title = document.createElement('h3'); title.textContent = t('admin.roles');
  const count = document.createElement('span'); count.className = 'badge'; count.textContent = t('roles.count', { count: roles.length });
  heading.append(title, count);

  const form = document.createElement('form'); form.className = 'role-form';
  let editingRole = null;
  const nameLabel = document.createElement('label'); nameLabel.textContent = t('roles.roleName');
  const name = document.createElement('input'); name.id = 'admin-role-name'; name.required = true; name.maxLength = 63; name.autocomplete = 'off'; name.placeholder = 'reporting_reader'; nameLabel.append(name);
  const expirationLabel = document.createElement('label'); expirationLabel.textContent = t('roles.validUntil');
  const expiration = document.createElement('input'); expiration.id = 'admin-role-valid-until'; expiration.type = 'date'; expirationLabel.append(expiration);
  const limitLabel = document.createElement('label'); limitLabel.textContent = t('roles.connectionLimit');
  const limit = document.createElement('input'); limit.id = 'admin-role-limit'; limit.type = 'number'; limit.min = '-1'; limit.value = '-1'; limitLabel.append(limit);
  const options = document.createElement('div'); options.className = 'role-options';
  const loginOption = roleCheckbox('admin-role-login', t('roles.canLogin'));
  const createDbOption = roleCheckbox('admin-role-createdb', t('roles.createDatabases'));
  const createRoleOption = roleCheckbox('admin-role-createrole', t('roles.createRoles'));
  const superuserOption = roleCheckbox('admin-role-superuser', t('roles.superuser'));
  options.append(loginOption, createDbOption, createRoleOption, superuserOption);
  const login = loginOption.querySelector('input');
  const createDatabase = createDbOption.querySelector('input');
  const createRole = createRoleOption.querySelector('input');
  const superuser = superuserOption.querySelector('input');
  const formActions = document.createElement('div'); formActions.className = 'form-actions role-form-actions';
  const submit = document.createElement('button'); submit.className = 'button primary'; submit.type = 'submit'; submit.textContent = t('roles.createRole');
  const cancelEdit = document.createElement('button'); cancelEdit.className = 'button'; cancelEdit.type = 'button'; cancelEdit.textContent = t('cron.cancelEdit'); cancelEdit.hidden = true;
  formActions.append(submit, cancelEdit);
  const status = document.createElement('div'); status.className = 'form-status role-status'; status.setAttribute('role', 'status');
  form.append(nameLabel, expirationLabel, limitLabel, options, formActions, status);
  const resetForm = () => {
    editingRole = null;
    form.reset();
    limit.value = '-1';
    name.disabled = false;
    submit.textContent = t('roles.createRole');
    cancelEdit.hidden = true;
    status.textContent = '';
    status.className = 'form-status role-status';
  };
  cancelEdit.addEventListener('click', resetForm);
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    const input = { login: login.checked, create_database: createDatabase.checked, create_role: createRole.checked, superuser: superuser.checked, connection_limit: Number(limit.value), valid_until: expiration.value || null };
    if (editingRole && !await showConfirm(t('roles.applyConfirm', { name: editingRole }), t('roles.updateRole'), false, t('cron.update'))) return;
    submit.disabled = true; status.textContent = editingRole ? t('roles.updatingRole') : t('roles.creatingRole'); status.className = 'form-status role-status';
    try {
      if (editingRole) await invoke('update_role', { id, name: editingRole, input });
      else await invoke('create_role', { id, input: { name: name.value.trim(), ...input } });
      await loadAdmin(id);
    } catch (error) {
      submit.disabled = false; status.textContent = (error?.key ? errorMessage(error, t) : error?.message || (editingRole ? t('roles.couldNotUpdateRole') : t('roles.couldNotCreateRole'))); status.className = 'form-status role-status error';
    }
  });

  const list = document.createElement('div'); list.className = 'role-list';
  for (const role of roles) {
    const row = document.createElement('div'); row.className = 'role-item';
    const detail = document.createElement('div');
    const roleName = document.createElement('strong'); roleName.textContent = role.name;
    const attributes = document.createElement('small'); attributes.textContent = roleAttributes(role);
    detail.append(roleName, attributes);
    const actions = document.createElement('div'); actions.className = 'role-actions';
    const edit = document.createElement('button'); edit.className = 'button small'; edit.type = 'button'; edit.textContent = t('common.edit');
    const remove = document.createElement('button'); remove.className = 'button small danger'; remove.type = 'button'; remove.textContent = t('common.delete');
    const reserved = role.name.toLowerCase().startsWith('pg_');
    edit.disabled = reserved; edit.title = reserved ? t('roles.postgresqlSystemRolesCannotBe2') : t('table.editNamed', { name: role.name });
    remove.disabled = reserved; remove.title = reserved ? t('roles.postgresqlSystemRolesCannotBe') : t('roles.deleteNamed', { name: role.name });
    edit.addEventListener('click', () => {
      editingRole = role.name;
      name.value = role.name;
      name.disabled = true;
      expiration.value = /^\d{4}-\d{2}-\d{2}$/.test(role.valid_until || '') ? role.valid_until : '';
      limit.value = String(role.connection_limit);
      login.checked = role.login;
      createDatabase.checked = role.create_database;
      createRole.checked = role.create_role;
      superuser.checked = role.superuser;
      submit.textContent = t('cron.saveChanges');
      cancelEdit.hidden = false;
      status.textContent = t('roles.editing', { name: role.name });
      form.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    });
    remove.addEventListener('click', async () => {
      const confirmation = await showDangerPrompt(t('roles.deleteConfirm', { name: role.name }), t('roles.deleteRole'), t('roles.roleName'), role.name);
      if (confirmation !== role.name) { if (confirmation !== null) await showAlert('The role name did not match. Nothing was deleted.', t('roles.roleNotDeleted')); return; }
      try { await invoke('delete_role', { id, name: role.name }); await loadAdmin(id); }
      catch (error) { await showAlert(errorMessage(error, t, 'roles.couldNotDeleteRole'), t('roles.roleNotDeleted')); }
    });
    actions.append(edit, remove);
    row.append(detail, actions); list.append(row);
  }
  panel.append(heading, form, list);
  return panel;
}

function renderActivityPanel(id, rows) {
  const panel = document.createElement('section'); panel.className = 'data-panel activity-panel';
  const heading = document.createElement('h3'); heading.textContent = t('admin.activity'); panel.append(heading);
  if (!rows.length) { panel.append(errorState(t('activity.noOtherSessions'), t('activity.postgresqlReportedNoActivityFor'))); return panel; }
  for (const activity of rows.slice(0, 20)) {
    const row = document.createElement('div'); row.className = 'activity-item'; row.dataset.pid = String(activity.pid); row.tabIndex = -1;
    const detail = document.createElement('div');
    const title = document.createElement('strong'); title.textContent = `PID ${activity.pid} · ${activity.usename || t('activity.unknownUser')} · ${activity.state || t('activity.unknown')}`;
    const query = document.createElement('small'); query.textContent = activity.query || t('activity.noCurrentQuery'); query.title = activity.query || '';
    detail.append(title, query);
    const cancel = document.createElement('button'); cancel.className = 'button small danger'; cancel.type = 'button'; cancel.textContent = t('activity.cancelQuery');
    const active = activity.state === 'active'; cancel.disabled = !active; cancel.title = active ? t('activity.cancelOnPid', { pid: activity.pid }) : t('activity.onlyActiveQueriesCanBe');
    cancel.addEventListener('click', async () => {
      if (!await showConfirm(t('activity.cancelConfirm', { pid: activity.pid }), t('activity.cancelActiveQuery'), true, t('activity.cancelQuery'))) return;
      try { await invoke('cancel_activity', { id, pid: activity.pid }); await loadAdmin(id); }
      catch (error) { await showAlert(errorMessage(error, t, 'activity.couldNotCancelTheQuery'), t('activity.queryNotCancelled')); }
    });
    row.append(detail, cancel); panel.append(row);
  }
  return panel;
}

function renderLocksPanel(rows) {
  const panel = document.createElement('section'); panel.className = 'data-panel locks-panel';
  const heading = document.createElement('h3'); heading.textContent = t('admin.locks'); panel.append(heading);
  if (!rows.length) { panel.append(errorState(t('locks.noBlockingLocks'), t('locks.postgresqlReportedNoBlockedSessions'))); return panel; }
  for (const lock of rows.slice(0, 20)) {
    const row = document.createElement('div'); row.className = 'activity-item';
    const detail = document.createElement('div');
    const title = document.createElement('strong'); title.textContent = t('locks.blockedBy', { blocked: lock.blocked_pid, blocking: lock.blocking_pid });
    const metadata = document.createElement('small'); metadata.textContent = `${lock.locktype || 'lock'} · ${lock.wait_sec ? `${lock.wait_sec}s` : t('locks.waiting')} · ${lock.blocking_user || t('activity.unknownUser')}`;
    detail.append(title, metadata);
    const locate = document.createElement('button'); locate.className = 'button small'; locate.type = 'button'; locate.textContent = t('locks.showBlocker');
    locate.addEventListener('click', async () => {
      const activity = document.querySelector(`.activity-item[data-pid="${lock.blocking_pid}"]`);
      if (!activity) { await showAlert(t('locks.blockerGone', { pid: lock.blocking_pid }), t('locks.blockingSessionChanged')); return; }
      showAdminPanel('activity');
      activity.classList.add('located'); activity.focus(); activity.scrollIntoView({ behavior: 'smooth', block: 'center' });
      window.setTimeout(() => activity.classList.remove('located'), 2400);
    });
    row.append(detail, locate); panel.append(row);
  }
  return panel;
}

function renderCronJobsPanel(id, cron) {
  const panel = document.createElement('section'); panel.className = 'data-panel cron-panel';
  const heading = document.createElement('div'); heading.className = 'roles-heading';
  const title = document.createElement('h3'); title.textContent = t('cron.scheduledJobsPgCron');
  const status = document.createElement('span'); status.className = 'badge'; status.textContent = cron.installed ? t('cron.count', { count: cron.jobs.length }) : t('cron.notInstalled');
  heading.append(title, status); panel.append(heading);
  if (!cron.installed) { panel.append(errorState('pg_cron is not installed', t('cron.installAndPreloadPgCron'))); return panel; }

  const form = document.createElement('form'); form.className = 'cron-form';
  let editingJob = null;
  const nameLabel = document.createElement('label'); nameLabel.textContent = t('cron.nameOptional');
  const jobName = document.createElement('input'); jobName.maxLength = 63; jobName.placeholder = 'nightly_cleanup'; nameLabel.append(jobName);
  const scheduleLabel = document.createElement('label'); scheduleLabel.textContent = t('cron.schedule');
  const schedule = document.createElement('input'); schedule.required = true; schedule.maxLength = 100; schedule.placeholder = t('cron.02OrDaily'); scheduleLabel.append(schedule);
  const commandLabel = document.createElement('label'); commandLabel.textContent = t('cron.sqlCommand');
  const commandInput = document.createElement('textarea'); commandInput.required = true; commandInput.maxLength = 100000; commandInput.rows = 2; commandInput.placeholder = 'VACUUM ANALYZE public.events;'; commandLabel.append(commandInput);
  const formActions = document.createElement('div'); formActions.className = 'form-actions cron-form-actions';
  const submit = document.createElement('button'); submit.className = 'button primary'; submit.type = 'submit'; submit.textContent = t('cron.createJob');
  const cancelEdit = document.createElement('button'); cancelEdit.className = 'button'; cancelEdit.type = 'button'; cancelEdit.textContent = t('cron.cancelEdit'); cancelEdit.hidden = true;
  formActions.append(submit, cancelEdit);
  const formStatus = document.createElement('div'); formStatus.className = 'form-status cron-form-status'; formStatus.setAttribute('role', 'status');
  form.append(nameLabel, scheduleLabel, commandLabel, formActions, formStatus);
  const resetForm = () => {
    editingJob = null;
    form.reset();
    jobName.disabled = false;
    submit.textContent = t('cron.createJob');
    cancelEdit.hidden = true;
    formStatus.textContent = '';
    formStatus.className = 'form-status cron-form-status';
  };
  cancelEdit.addEventListener('click', resetForm);
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    const input = { name: jobName.value.trim() || null, schedule: schedule.value.trim(), command: commandInput.value.trim() };
    if (editingJob && !await showConfirm(t('cron.updateConfirm', { name: editingJob.name || t('cron.jobNumber', { id: editingJob.id }) }), t('cron.updateScheduledJob'), false, t('cron.update'))) return;
    submit.disabled = true;
    formStatus.textContent = editingJob ? t('cron.updatingJob') : t('cron.creatingJob');
    try {
      if (editingJob) await invoke('update_cron_job', { id, jobId: editingJob.id, input });
      else await invoke('create_cron_job', { id, input });
      await loadAdmin(id);
    } catch (error) {
      submit.disabled = false;
      formStatus.textContent = (error?.key ? errorMessage(error, t) : error?.message || (editingJob ? t('cron.couldNotUpdateTheJob') : t('cron.couldNotCreateTheJob')));
      formStatus.className = 'form-status cron-form-status error';
    }
  });
  panel.append(form);
  if (!cron.jobs.length) { panel.append(errorState(t('cron.noScheduledJobs'), 'pg_cron is available. Use the form above to create the first job.')); return panel; }
  for (const job of cron.jobs) {
    const row = document.createElement('div'); row.className = 'cron-item';
    const detail = document.createElement('div');
    const name = document.createElement('strong'); name.textContent = job.name || t('cron.jobNumber', { id: job.id });
    const metadata = document.createElement('small'); metadata.textContent = `${job.schedule} · ${job.active ? t('cron.active') : t('cron.paused')} · ${t('cron.last', { status: job.last_status || t('cron.neverRun') })}${job.last_run ? ` · ${job.last_run} UTC` : ''}`;
    const command = document.createElement('code'); command.textContent = job.command; command.title = job.command;
    detail.append(name, metadata, command);
    const actions = document.createElement('div'); actions.className = 'cron-actions';
    const edit = document.createElement('button'); edit.className = 'button small'; edit.type = 'button'; edit.textContent = t('common.edit');
    edit.addEventListener('click', () => {
      editingJob = job;
      jobName.value = job.name || '';
      jobName.disabled = true;
      schedule.value = job.schedule;
      commandInput.value = job.command;
      submit.textContent = t('cron.saveChanges');
      cancelEdit.hidden = false;
      formStatus.textContent = t('roles.editing', { name: job.name || t('cron.jobNumber', { id: job.id }) });
      form.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    });
    const history = document.createElement('button'); history.className = 'button small'; history.type = 'button'; history.textContent = t('cron.runs');
    history.addEventListener('click', async () => {
      history.disabled = true;
      try {
        const runs = await invoke('cron_job_runs', { id, jobId: job.id });
        const summary = runs.length ? runs.map((run) => `#${run.id} · ${run.status || t('cron.unknownStatus')} · ${run.start_time || t('cron.noStartTime')} UTC${run.duration_seconds ? ` · ${run.duration_seconds}s` : ''}${run.return_message ? `\n${run.return_message}` : ''}`).join('\n\n') : t('cron.noExecutionsWereRecordedFor');
        await showAlert(summary, t('cron.runsTitle', { name: job.name || t('cron.jobNumber', { id: job.id }) }));
      } catch (error) {
        await showAlert(errorMessage(error, t, 'cron.couldNotLoadJobExecutions'), t('cron.jobHistoryUnavailable'));
      } finally {
        history.disabled = false;
      }
    });
    const toggle = document.createElement('button'); toggle.className = 'button small'; toggle.type = 'button'; toggle.textContent = job.active ? t('cron.pause') : t('cron.resume');
    toggle.addEventListener('click', async () => {
      toggle.disabled = true;
      try { await invoke('set_cron_job_active', { id, jobId: job.id, active: !job.active }); await loadAdmin(id); }
      catch (error) { toggle.disabled = false; await showAlert(errorMessage(error, t, 'cron.couldNotChangeTheJob'), t('cron.jobNotChanged')); }
    });
    const remove = document.createElement('button'); remove.className = 'button small danger'; remove.type = 'button'; remove.textContent = t('common.delete');
    remove.addEventListener('click', async () => {
      if (!await showConfirm(t('cron.deleteConfirm', { name: job.name || job.id }), t('cron.deleteScheduledJob'), true)) return;
      try { await invoke('delete_cron_job', { id, jobId: job.id }); await loadAdmin(id); }
      catch (error) { await showAlert(errorMessage(error, t, 'cron.couldNotDeleteTheScheduled'), t('cron.jobNotDeleted')); }
    });
    actions.append(edit, history, toggle, remove); row.append(detail, actions); panel.append(row);
  }
  return panel;
}

function renderExtensionsPanel(id, extensions) {
  const panel = document.createElement('section'); panel.className = 'data-panel extensions-panel';
  const heading = document.createElement('div'); heading.className = 'roles-heading';
  const title = document.createElement('h3'); title.textContent = t('admin.extensions');
  const count = document.createElement('span'); count.className = 'badge'; count.textContent = t('extensions.installedCount', { count: extensions.installed.length });
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
    const metadata = document.createElement('small'); metadata.textContent = extension.installed ? t('extensions.installedVersion', { version: extension.installed_version || '' }) : t('extensions.availableVersion', { version: extension.default_version || '' }); metadata.title = extension.comment || '';
    detail.append(name, metadata);
    const action = document.createElement('button'); action.className = `button small ${extension.installed ? 'danger' : ''}`; action.type = 'button'; action.textContent = extension.installed ? t('extensions.drop') : t('extensions.install');
    const protectedExtension = extension.name.toLowerCase() === 'plpgsql';
    action.disabled = protectedExtension; action.title = protectedExtension ? t('extensions.theBuiltInPlpgsqlExtension') : `${action.textContent} ${extension.name}`;
    action.addEventListener('click', async () => {
      if (extension.installed) {
        const confirmation = await showDangerPrompt(t('extensions.dropConfirm', { name: extension.name }), t('extensions.dropExtension'), t('extensions.extensionName'), extension.name);
        if (confirmation !== extension.name) { if (confirmation !== null) await showAlert('The extension name did not match. Nothing was removed.', t('extensions.extensionNotDropped')); return; }
        try { await invoke('drop_extension', { id, name: extension.name }); await loadAdmin(id); }
        catch (error) { await showAlert(errorMessage(error, t, 'extensions.couldNotDropTheExtension'), t('extensions.extensionNotDropped')); }
      } else {
        if (!await showConfirm(t('extensions.installConfirm', { name: extension.name }), t('extensions.installExtension'), false, t('extensions.install'))) return;
        try { await invoke('install_extension', { id, name: extension.name }); await loadAdmin(id); }
        catch (error) { await showAlert(errorMessage(error, t, 'extensions.couldNotInstallTheExtension'), t('extensions.extensionNotInstalled')); }
      }
    });
    row.append(detail, action); panel.append(row);
  }
  return panel;
}

function renderQueryStatsPanel(id, stats, sortBy = 'total') {
  const panel = document.createElement('section'); panel.className = 'data-panel query-stats-panel';
  const heading = document.createElement('div'); heading.className = 'query-stats-heading';
  const title = document.createElement('h3'); title.textContent = t('stats.queryStatsPgStatStatements');
  const controls = document.createElement('div'); controls.className = 'query-stats-controls';
  const sort = document.createElement('select'); sort.setAttribute('aria-label', t('stats.sortQueryStatistics'));
  for (const [value, label] of [['total', t('stats.totalTime')], ['calls', t('stats.calls')], ['mean', t('stats.meanTime')]]) { const option = document.createElement('option'); option.value = value; option.textContent = label; sort.append(option); }
  sort.value = sortBy; sort.disabled = !stats.installed || !stats.queries.length;
  const reset = document.createElement('button'); reset.className = 'button small danger'; reset.type = 'button'; reset.textContent = t('stats.resetStats'); reset.disabled = !stats.installed;
  reset.addEventListener('click', async () => {
    if (!await showConfirm(t('stats.resetAllPgStatStatements'), t('stats.resetQueryStatistics'), true, t('explorer.sequenceReset'))) return;
    try { await invoke('reset_query_stats', { id }); await loadAdmin(id); }
    catch (error) { await showAlert(errorMessage(error, t, 'stats.couldNotResetQueryStatistics'), t('stats.statisticsNotReset')); }
  });
  controls.append(sort, reset); heading.append(title, controls); panel.append(heading);
  if (!stats.installed) { panel.append(errorState('pg_stat_statements is not installed', t('stats.installItFromExtensionsPostgresql'))); return panel; }
  if (!stats.queries.length) { panel.append(errorState(t('stats.noQueryStatistics'), t('stats.theExtensionIsActiveBut'))); return panel; }
  const sortKey = { total: 'total_exec_ms', calls: 'calls', mean: 'mean_exec_ms' }[sortBy] || 'total_exec_ms';
  const rows = [...stats.queries].sort((left, right) => Number(right[sortKey]) - Number(left[sortKey]));
  for (const queryStat of rows) {
    const row = document.createElement('div'); row.className = 'query-stat-item';
    const detail = document.createElement('div');
    const query = document.createElement('code'); query.textContent = queryStat.query; query.title = queryStat.query;
    const metadata = document.createElement('small'); metadata.textContent = t('stats.summary', { calls: queryStat.calls, mean: queryStat.mean_exec_ms.toFixed(1), total: queryStat.total_exec_ms.toFixed(1), rows: queryStat.rows });
    detail.append(query, metadata);
    const actions = document.createElement('div'); actions.className = 'query-stat-actions';
    const open = document.createElement('button'); open.className = 'button small'; open.type = 'button'; open.textContent = t('stats.openQuery'); open.addEventListener('click', () => openSqlInNewTab(queryStat.query, id));
    const analyze = document.createElement('button'); analyze.className = 'button small'; analyze.type = 'button'; analyze.textContent = t('stats.askAssistant'); analyze.addEventListener('click', () => void askAssistantAboutQueryStat(id, queryStat));
    actions.append(open, analyze); row.append(detail, actions); panel.append(row);
  }
  sort.addEventListener('change', () => {
    const replacement = renderQueryStatsPanel(id, stats, sort.value);
    replacement.dataset.adminPanel = 'query-stats';
    panel.replaceWith(replacement);
  });
  return panel;
}

const REPLICATION_REFRESH_MS = 5000;
let replicationRefreshTimer = null;
let replicationAutoRefresh = true;

function stopReplicationRefresh() {
  if (replicationRefreshTimer) window.clearInterval(replicationRefreshTimer);
  replicationRefreshTimer = null;
}

// Refreshes only while the Replication tab of this connection is on screen, like a monitor; a
// failed refresh keeps the last panel and says so instead of blanking it.
function scheduleReplicationRefresh(id) {
  stopReplicationRefresh();
  if (!replicationAutoRefresh) return;
  replicationRefreshTimer = window.setInterval(async () => {
    if (byId('view-admin').hidden || currentAdminPanel !== 'replication' || byId('admin-connection').value !== id) return;
    const current = byId('admin-content').querySelector('[data-admin-panel="replication"]');
    if (!current) { stopReplicationRefresh(); return; }
    try {
      const status = await invoke('replication_status', { id });
      if (byId('admin-connection').value !== id || !current.isConnected) return;
      const replacement = renderReplicationPanel(id, status);
      replacement.dataset.adminPanel = 'replication';
      replacement.hidden = current.hidden;
      current.replaceWith(replacement);
    } catch {
      const stamp = current.querySelector('.replication-updated');
      if (stamp) { stamp.textContent = t('replication.refreshFailed'); stamp.classList.add('error'); }
    }
  }, REPLICATION_REFRESH_MS);
}

function replicationMetric(label, value, warn = false) {
  const item = document.createElement('div'); item.className = `replication-metric${warn ? ' warn' : ''}`;
  const key = document.createElement('span'); key.textContent = label;
  const text = document.createElement('strong'); text.textContent = value;
  item.append(key, text);
  return item;
}

function replicationBadge(text, tone = '') {
  const badge = document.createElement('span'); badge.className = `badge${tone ? ` ${tone}` : ''}`; badge.textContent = text;
  return badge;
}

function renderReplicationPanel(id, status) {
  const locale = t.locale;
  const bytes = (value) => formatWalBytes(value, locale);
  const seconds = (value) => formatLagSeconds(value, locale);
  const mode = replicationMode(status);
  const panel = document.createElement('section'); panel.className = 'data-panel replication-panel';
  const heading = document.createElement('div'); heading.className = 'query-stats-heading';
  const title = document.createElement('h3'); title.textContent = t('replication.title');
  title.append(' ', replicationBadge(t(status.in_recovery ? 'replication.roleStandby' : 'replication.rolePrimary')));
  const controls = document.createElement('div'); controls.className = 'query-stats-controls';
  const updated = document.createElement('small'); updated.className = 'replication-updated';
  updated.textContent = t('replication.updatedAt', { time: new Date().toLocaleTimeString(locale) });
  const auto = document.createElement('label'); auto.className = 'check-row replication-auto';
  const autoInput = document.createElement('input'); autoInput.type = 'checkbox'; autoInput.checked = replicationAutoRefresh;
  const autoText = document.createElement('span'); autoText.textContent = t('replication.autoRefresh');
  auto.append(autoInput, autoText);
  autoInput.addEventListener('change', () => { replicationAutoRefresh = autoInput.checked; if (replicationAutoRefresh) scheduleReplicationRefresh(id); else stopReplicationRefresh(); });
  const refresh = document.createElement('button'); refresh.className = 'button small'; refresh.type = 'button'; refresh.textContent = t('replication.refresh');
  refresh.addEventListener('click', async () => {
    refresh.disabled = true;
    try {
      const next = renderReplicationPanel(id, await invoke('replication_status', { id }));
      next.dataset.adminPanel = 'replication'; next.hidden = panel.hidden; panel.replaceWith(next);
      next.querySelector('button')?.focus();
    } catch (error) {
      updated.textContent = errorMessage(error, t, 'replication.refreshFailed'); updated.classList.add('error'); refresh.disabled = false;
    }
  });
  controls.append(updated, auto, refresh); heading.append(title, controls); panel.append(heading);

  if (!status.has_monitor_privilege) {
    const note = document.createElement('p'); note.className = 'replication-note'; note.textContent = t('replication.needsMonitor');
    panel.append(note);
  }

  if (mode === 'none') {
    panel.append(errorState(t('replication.noneTitle'), t('replication.noneMessage')));
    return panel;
  }

  if (mode === 'standby') {
    const standby = status.standby || {};
    const section = document.createElement('div'); section.className = 'replication-section';
    const subtitle = document.createElement('h4'); subtitle.textContent = t('replication.standbyTitle');
    const grid = document.createElement('div'); grid.className = 'replication-metrics';
    const sender = standby.sender_host ? `${standby.sender_host}${standby.sender_port ? `:${standby.sender_port}` : ''}` : '—';
    grid.append(
      replicationMetric(t('replication.receiverStatus'), standby.receiver_status || t('replication.receiverStopped'), !standby.receiver_status),
      replicationMetric(t('replication.upstream'), sender),
      replicationMetric(t('replication.receiveLsn'), standby.receive_lsn || '—'),
      replicationMetric(t('replication.replayLsn'), standby.replay_lsn || '—'),
      replicationMetric(t('replication.replayBacklog'), bytes(standby.replay_backlog_bytes), Number(standby.replay_backlog_bytes) > 16 * 1024 * 1024),
      replicationMetric(t('replication.lastReplay'), standby.last_replay_at || '—'),
      replicationMetric(t('replication.sinceLastReplay'), seconds(standby.seconds_since_last_replay)),
    );
    const hint = document.createElement('small'); hint.className = 'replication-hint'; hint.textContent = t('replication.sinceLastReplayHint');
    section.append(subtitle, grid, hint); panel.append(section);
  } else {
    const section = document.createElement('div'); section.className = 'replication-section';
    const subtitle = document.createElement('h4'); subtitle.textContent = t('replication.replicasTitle', { count: status.replicas.length });
    section.append(subtitle);
    if (!status.replicas.length) section.append(errorState(t('replication.noReplicas'), t('replication.noReplicasMessage')));
    for (const replica of status.replicas) {
      const row = document.createElement('article'); row.className = 'replication-item';
      const header = document.createElement('div'); header.className = 'replication-item-header';
      const name = document.createElement('strong'); name.textContent = replica.application_name || `PID ${replica.pid}`;
      const where = document.createElement('small'); where.textContent = [replica.client_addr, replica.usename, `PID ${replica.pid}`].filter(Boolean).join(' · ');
      const badges = document.createElement('span'); badges.className = 'replication-badges';
      badges.append(replicationBadge(replica.state || '—', replica.state === 'streaming' ? 'success' : 'warn'));
      if (replica.sync_state) badges.append(replicationBadge(replica.sync_state));
      header.append(name, where, badges);
      const grid = document.createElement('div'); grid.className = 'replication-metrics';
      grid.append(
        replicationMetric(t('replication.writeLag'), `${bytes(replica.write_lag_bytes)} · ${seconds(replica.write_lag_seconds)}`),
        replicationMetric(t('replication.flushLag'), `${bytes(replica.flush_lag_bytes)} · ${seconds(replica.flush_lag_seconds)}`),
        replicationMetric(t('replication.replayLag'), `${bytes(replica.replay_lag_bytes)} · ${seconds(replica.replay_lag_seconds)}`, Number(replica.replay_lag_bytes) > 16 * 1024 * 1024),
        replicationMetric(t('replication.sentLsn'), replica.sent_lsn || '—'),
        replicationMetric(t('replication.replayLsn'), replica.replay_lsn || '—'),
      );
      row.append(header, grid); section.append(row);
    }
    panel.append(section);
  }

  const slots = document.createElement('div'); slots.className = 'replication-section';
  const slotsTitle = document.createElement('h4'); slotsTitle.textContent = t('replication.slotsTitle', { count: status.slots.length });
  slots.append(slotsTitle);
  if (!status.slots.length) slots.append(errorState(t('replication.noSlots'), t('replication.noSlotsMessage')));
  for (const slot of status.slots) {
    const attention = slotNeedsAttention(slot);
    const row = document.createElement('div'); row.className = `activity-item replication-slot${attention ? ' warn' : ''}`;
    const detail = document.createElement('div');
    const name = document.createElement('strong'); name.textContent = slot.slot_name;
    const meta = document.createElement('small');
    meta.textContent = [slot.slot_type, slot.plugin, slot.database, t('replication.retained', { size: bytes(slot.retained_wal_bytes) }), slot.wal_status].filter(Boolean).join(' · ');
    detail.append(name, meta);
    const badges = document.createElement('span'); badges.className = 'replication-badges';
    badges.append(replicationBadge(slot.active ? t('replication.slotActive') : t('replication.slotInactive'), slot.active ? 'success' : 'warn'));
    if (attention) badges.append(replicationBadge(slot.wal_status === 'lost' ? t('replication.slotLost') : t('replication.slotHoldingWal'), 'error'));
    row.append(detail, badges); slots.append(row);
  }
  panel.append(slots);
  return panel;
}

let currentAdminPanel = 'roles';

function showAdminPanel(section) {
  currentAdminPanel = section;
  for (const button of byId('admin-tabs').querySelectorAll('[data-admin-tool]')) {
    const active = button.dataset.adminTool === section;
    button.classList.toggle('active', active);
    button.setAttribute('aria-selected', String(active));
  }
  for (const panel of byId('admin-content').querySelectorAll('[data-admin-panel]')) panel.hidden = panel.dataset.adminPanel !== section;
}

function renderAdminPanels(sections) {
  const tabs = byId('admin-tabs');
  const content = byId('admin-content');
  tabs.replaceChildren();
  content.replaceChildren();
  for (const [key, label, panel] of sections) {
    const tab = document.createElement('button');
    tab.className = 'admin-tool-tab';
    tab.type = 'button';
    tab.role = 'tab';
    tab.dataset.adminTool = key;
    tab.textContent = label;
    tab.addEventListener('click', () => showAdminPanel(key));
    panel.dataset.adminPanel = key;
    tabs.append(tab);
    content.append(panel);
  }
  tabs.hidden = false;
  showAdminPanel(sections.some(([key]) => key === currentAdminPanel) ? currentAdminPanel : sections[0][0]);
}

async function loadAdmin(id) {
  stopReplicationRefresh();
  const content = byId('admin-content'); content.replaceChildren();
  const tabs = byId('admin-tabs'); tabs.hidden = true; tabs.replaceChildren();
  if (!id) { content.append(errorState(t('admin.chooseAConnectedConnection'), t('admin.activityAndLocksAreRead'))); return; }
  content.append(errorState(t('admin.loadingAdministration'), ''));
  const [adminResult, rolesResult, cronResult, extensionsResult, queryStatsResult, replicationResult] = await Promise.allSettled([invoke('admin', { id }), invoke('list_roles', { id }), invoke('list_cron_jobs', { id }), invoke('list_extensions', { id }), invoke('query_stats', { id }), invoke('replication_status', { id })]);
  if (byId('admin-connection').value !== id) return;
  const sections = [
    ['roles', t('admin.roles'), rolesResult.status === 'fulfilled' ? renderRolesPanel(id, rolesResult.value) : unavailablePanel(t('admin.rolesUnavailable'), t('admin.theConnectedRoleMayNot'))],
    ['jobs', t('admin.scheduledJobs'), cronResult.status === 'fulfilled' ? renderCronJobsPanel(id, cronResult.value) : unavailablePanel(t('admin.scheduledJobsUnavailable'), t('admin.checkPgCronPermissionsAnd'))],
    ['extensions', t('admin.extensions'), extensionsResult.status === 'fulfilled' ? renderExtensionsPanel(id, extensionsResult.value) : unavailablePanel(t('admin.extensionsUnavailable'), t('admin.theConnectedRoleMayNot2'))],
    ['query-stats', t('admin.queryStats'), queryStatsResult.status === 'fulfilled' ? renderQueryStatsPanel(id, queryStatsResult.value) : unavailablePanel(t('admin.queryStatisticsUnavailable'), t('admin.checkPgStatStatementsConfiguration'))],
  ];
  if (adminResult.status === 'fulfilled') {
    const payload = adminResult.value;
    sections.push(['activity', t('admin.activity'), renderActivityPanel(id, payload.activity || [])]);
    sections.push(['locks', t('admin.locks'), renderLocksPanel(payload.locks || [])]);
  } else {
    sections.push(['activity', t('admin.activity'), unavailablePanel(t('admin.activityUnavailable'), t('admin.checkMonitoringPermissionsAndReconnect'))]);
    sections.push(['locks', t('admin.locks'), unavailablePanel(t('admin.locksUnavailable'), t('admin.checkMonitoringPermissionsAndReconnect'))]);
  }
  sections.push(['replication', t('admin.replication'), replicationResult.status === 'fulfilled' ? renderReplicationPanel(id, replicationResult.value) : unavailablePanel(t('replication.unavailable'), t('replication.unavailableMessage'))]);
  renderAdminPanels(sections);
  if (replicationResult.status === 'fulfilled') scheduleReplicationRefresh(id); else stopReplicationRefresh();
}

let explorerReturnState = null;

function rememberExplorerState() {
  if (byId('view-explorer').hidden) return;
  const workspace = document.querySelector('.workspace-content');
  explorerReturnState = {
    focus: document.activeElement instanceof HTMLElement ? document.activeElement : null,
    scrollTop: workspace?.scrollTop || 0,
  };
}

function restoreExplorerState() {
  byId('explorer-filter').value = state.explorerFilter;
  filterExplorerTree();
  const workspace = document.querySelector('.workspace-content');
  if (workspace && explorerReturnState) workspace.scrollTop = explorerReturnState.scrollTop;
  const focus = explorerReturnState?.focus;
  if (focus?.isConnected && !focus.hidden) focus.focus();
  else byId('explorer-filter').focus();
}

function returnToExplorer() {
  switchView('explorer');
  restoreExplorerState();
}

async function openTable(id, schema, table, kind = 'table') {
  rememberExplorerState();
  switchView('table-detail'); byId('detail-title').textContent = table; byId('detail-title').append(...connectionBadges(connectionById(id))); byId('detail-eyebrow').textContent = `${schema.toUpperCase()} · ${kind === 'view' ? t('table.viewDetailEyebrow') : t('table.tableDetailEyebrow')}`; byId('detail-summary').textContent = t('erd.loading');
  const content = byId('detail-content'); content.replaceChildren(errorState(t('table.loadingTableDetail'), ''));
  try {
    const payload = await invoke('table_detail', { id, schema, table }); const detail = payload.detail; content.replaceChildren(); byId('detail-summary').textContent = t('table.estimatedRows', { count: detail.row_estimate ?? 0 });
    if (kind === 'table' && !isReadOnly(id)) content.append(tableMaintenancePanel(id, schema, table, detail));
    content.append(tableDataPanel(id, schema, table, { importable: kind === 'table' }));
    content.append(dataPanel(t('table.columns'), (detail.columns || []).map((row) => [row.name, `${row.full_type || row.data_type}${row.is_nullable ? '' : ' · NOT NULL'}${row.is_primary_key ? ' · PK' : ''}`])));
    content.append(dataPanel(t('table.constraints'), (detail.constraints || []).map((row) => [row.name, `${row.type}: ${row.definition}`])));
    if (kind === 'table') content.append(indexPanel(id, schema, table, detail.indexes || []));
    content.append(foreignKeyMapPanel(id, detail.fk_map || []));
    content.append(dataPanel(t('table.columnStatistics'), (payload.column_stats || []).map((row) => [row.column, `${row.null_frac == null ? '—' : t('table.nullFraction', { percent: (row.null_frac * 100).toFixed(1) })} · ${row.n_distinct == null ? '—' : t('table.distinctValues', { count: row.n_distinct })}`])));
    content.append(codePanel('DDL', payload.ddl));
  } catch (error) { content.replaceChildren(errorState(t('table.tableDetailUnavailable'), t('table.theObjectMayHaveBeen'))); }
}

async function openErd() {
  const id = state.selectedConnectionId; const schema = state.selectedSchema; if (!id || !schema) return;
  switchView('erd'); byId('erd-title').textContent = `ERD · ${schema}`; byId('erd-title').append(...connectionBadges(connectionById(id))); byId('erd-summary').textContent = t('erd.loading');
  const content = byId('erd-content'); content.replaceChildren(errorState(t('erd.loadingErd'), ''));
  try {
    const payload = await invoke('erd', { id, schema }); const data = payload.data; content.replaceChildren(); byId('erd-summary').textContent = `${t('erd.tableCount', { count: (data.tables || []).length })} · ${t('erd.relationCount', { count: (data.relations || []).length })}`;
    content.append(renderErdCanvas(data));
    const relationsPanel = dataPanel(t('erd.relations'), (data.relations || []).map((row) => [`${row.from_table}.${row.from_column}`, `→ ${row.to_table}.${row.to_column}`]));
    relationsPanel.classList.add('erd-relations-panel');
    content.append(relationsPanel);
  } catch (error) { content.replaceChildren(errorState(t('erd.erdUnavailable'), t('erd.checkSchemaPermissionsAndReconnect'))); }
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
  const hint = document.createElement('span'); hint.className = 'query-hint'; hint.textContent = t('erd.dragToPanClickA');
  const controls = document.createElement('div'); controls.className = 'erd-controls';
  const zoomLabel = document.createElement('span'); zoomLabel.className = 'badge';
  const zoomOut = document.createElement('button'); zoomOut.className = 'button small'; zoomOut.type = 'button'; zoomOut.textContent = '−'; zoomOut.title = t('erd.zoomOut');
  const zoomIn = document.createElement('button'); zoomIn.className = 'button small'; zoomIn.type = 'button'; zoomIn.textContent = '+'; zoomIn.title = t('erd.zoomIn');
  const reset = document.createElement('button'); reset.className = 'button small'; reset.type = 'button'; reset.textContent = t('erd.resetView');
  controls.append(zoomOut, zoomLabel, zoomIn, reset); toolbar.append(hint, controls);
  const canvas = document.createElement('div'); canvas.className = 'erd-canvas'; canvas.tabIndex = 0; canvas.setAttribute('aria-label', t('erd.entityRelationshipDiagram'));
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
    if ((table.columns || []).length > 8) { const more = document.createElement('small'); more.className = 'erd-node-more'; more.textContent = t('erd.moreColumns', { count: table.columns.length - 8 }); list.append(more); }
    node.append(header, list);
    const select = () => { selectedTable = table.name; updateSelection(); byId('erd-summary').textContent = `${table.name} · ${t('erd.columnCount', { count: (table.columns || []).length })}`; };
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
const RESULT_DEFAULT_COLUMN_WIDTH = 180;
let resultVirtualCleanup = null;

function buildResultHeader(column, index, columnElement) {
  const cell = document.createElement('th');
  cell.className = 'result-column-header';
  const label = document.createElement('span');
  label.textContent = column;
  const resizer = document.createElement('span');
  resizer.className = 'result-column-resizer';
  resizer.tabIndex = 0;
  resizer.setAttribute('role', 'separator');
  resizer.setAttribute('aria-orientation', 'vertical');
  resizer.setAttribute('aria-label', t('results.resizeColumn', { column }));
  resizer.setAttribute('aria-valuemin', '80');
  resizer.setAttribute('aria-valuemax', '640');
  let pointerId = null;
  let startX = 0;
  let startWidth = RESULT_DEFAULT_COLUMN_WIDTH;
  const setWidth = (width) => {
    const next = clampResultColumnWidth(width);
    columnElement.style.width = `${next}px`;
    resizer.setAttribute('aria-valuenow', String(next));
  };
  resizer.addEventListener('pointerdown', (event) => {
    event.preventDefault();
    pointerId = event.pointerId;
    startX = event.clientX;
    startWidth = Number.parseFloat(columnElement.style.width) || RESULT_DEFAULT_COLUMN_WIDTH;
    resizer.setPointerCapture(pointerId);
    resizer.classList.add('resizing');
  });
  resizer.addEventListener('pointermove', (event) => {
    if (pointerId !== event.pointerId) return;
    setWidth(startWidth + event.clientX - startX);
  });
  const finishResize = (event) => {
    if (pointerId !== event.pointerId) return;
    pointerId = null;
    resizer.classList.remove('resizing');
  };
  resizer.addEventListener('pointerup', finishResize);
  resizer.addEventListener('pointercancel', finishResize);
  resizer.addEventListener('keydown', (event) => {
    if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
    event.preventDefault();
    const current = Number.parseFloat(columnElement.style.width) || RESULT_DEFAULT_COLUMN_WIDTH;
    setWidth(current + (event.key === 'ArrowRight' ? 16 : -16));
  });
  setWidth(RESULT_DEFAULT_COLUMN_WIDTH);
  cell.dataset.columnIndex = String(index);
  cell.append(label, resizer);
  return cell;
}

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
  detail.textContent = t('results.details');
  detail.setAttribute('aria-label', t('results.detailsFor', { row: rowIndex + 1 }));
  detail.addEventListener('click', () => {
    void showAlert(resultRowToText(row, result.columns), t('results.rowNumber', { row: rowIndex + 1 }));
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
    byId('result-summary').textContent = t('results.none');
    delete byId('result-summary').dataset.rows;
    grid.replaceChildren();
    const empty = document.createElement('div');
    empty.className = 'empty-state';
    empty.textContent = t('results.emptyTitle');
    grid.append(empty);
    return;
  }
  byId('result-summary').textContent = t('results.summary', { count: result.rows.length, ms: result.duration_ms });
  byId('result-summary').dataset.rows = String(result.rows.length);
  grid.replaceChildren();
  if (!result.columns.length) {
    const empty = document.createElement('div');
    empty.className = 'empty-state';
    empty.textContent = t('results.noRows');
    grid.append(empty);
    return;
  }
  const table = document.createElement('table');
  const columnGroup = document.createElement('colgroup');
  const head = document.createElement('thead');
  const headerRow = document.createElement('tr');
  for (const [index, column] of result.columns.entries()) {
    const columnElement = document.createElement('col');
    columnGroup.append(columnElement);
    headerRow.append(buildResultHeader(column, index, columnElement));
  }
  const actionColumn = document.createElement('col');
  actionColumn.className = 'result-action-column';
  columnGroup.append(actionColumn);
  const actionHeader = document.createElement('th');
  actionHeader.textContent = t('results.row');
  headerRow.append(actionHeader);
  head.append(headerRow);
  const body = document.createElement('tbody');
  table.append(columnGroup, head, body);
  grid.append(table);
  resultVirtualCleanup = renderVirtualizedRows(body, result, result.columns.length + 1);
}

async function runQuery(mode = 'query') {
  const script = mode === 'script';
  const explain = mode === 'explain';
  const id = byId('query-connection').value;
  const sql = script ? byId('sql-editor').value : selectedQueryText();
  const selectionActive = !script && byId('sql-editor').selectionStart !== byId('sql-editor').selectionEnd;
  if (!id) { byId('query-status').textContent = t('common.selectConnected'); return; }
  if (!sql.trim()) { byId('query-status').textContent = t('query.empty'); return; }
  saveCurrentQueryTab();
  state.currentQueryId = id;
  state.currentQueryOperationId = operationId();
  state.cancelRequested = false;
  byId('run-query').disabled = true;
  byId('run-script').disabled = true;
  byId('explain-query').disabled = true;
  byId('cancel-query').hidden = false;
  byId('query-status').textContent = explain ? t('query.planning') : script ? t('query.runningScript') : selectionActive ? t('query.runningSelection') : t('query.running');
  byId('result-error').textContent = '';
  const operationKind = explain ? 'explain' : script ? 'script' : 'query';
  const startedAt = performance.now();
  try {
    const command = explain ? 'execute_explain' : script ? 'execute_script' : 'execute_query';
    const result = await invoke(command, { id, sql, operationId: state.currentQueryOperationId });
    renderResult(result);
    byId('query-status').textContent = explain ? t('query.planReady') : t('query.completed');
    reportOperationFinished(operationKind, 'succeeded', startedAt);
  } catch (error) {
    if (error?.code !== 'write_cancelled') reportOperationFinished(operationKind, state.cancelRequested ? 'cancelled' : 'failed', startedAt);
    renderResult(null);
    byId('result-error').textContent = state.cancelRequested ? t('query.cancelledDetail') : errorMessage(error, t, explain ? 'query.explainFailed' : 'query.failed');
    byId('query-status').textContent = state.cancelRequested ? t('query.cancelled') : t('query.error');
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
  byId('query-status').textContent = t('query.cancelling');
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
    byId('query-status').textContent = t('results.copied');
  } catch {
    await showAlert(t('results.copyFailed'), t('results.copyUnavailable'));
  }
}

async function loadHistory() {
  const list = byId('history-list');
  list.replaceChildren();
  const entries = await invoke('list_history');
  if (!entries.length) { const empty = document.createElement('div'); empty.className = 'empty-state'; empty.textContent = t('history.empty'); list.append(empty); return; }
  for (const entry of entries) {
    const item = document.createElement('article'); item.className = 'history-item';
    const sql = document.createElement('code'); sql.textContent = entry.sql;
    const meta = document.createElement('small'); meta.textContent = `${entry.conn_label} · ${t('results.summary', { count: entry.row_count, ms: entry.duration_ms })}`;
    const remove = document.createElement('button'); remove.className = 'button small danger history-meta'; remove.type = 'button'; remove.textContent = t('common.delete');
    remove.addEventListener('click', async (event) => { event.stopPropagation(); await invoke('delete_history_entry', { id: entry.id }); loadHistory(); });
    item.append(sql, meta, remove); item.addEventListener('click', () => { setEditorValue(entry.sql); switchView('query'); showQueryWorkspaceSection('editor'); }); list.append(item);
  }
}

async function loadSnippets() {
  const list = byId('snippet-list'); list.replaceChildren();
  const snippets = await invoke('list_snippets');
  if (!snippets.length) { const empty = document.createElement('div'); empty.className = 'empty-state'; empty.textContent = t('snippets.empty'); list.append(empty); return; }
  for (const snippet of snippets) {
    const item = document.createElement('article'); item.className = 'snippet-item';
    const name = document.createElement('strong'); name.textContent = snippet.name;
    const sql = document.createElement('code'); sql.textContent = snippet.sql;
    const meta = document.createElement('small'); meta.textContent = snippet.conn_label || t('snippets.allConnections');
    const actions = document.createElement('div'); actions.className = 'snippet-meta snippet-actions';
    const rename = document.createElement('button'); rename.className = 'button small'; rename.type = 'button'; rename.textContent = t('snippets.rename');
    rename.addEventListener('click', async (event) => {
      event.stopPropagation();
      const nextName = await showPrompt(t('snippets.renameMessage'), t('snippets.renameTitle'), t('snippets.name'), '', snippet.name);
      if (!nextName || nextName === snippet.name) return;
      try { await invoke('rename_snippet', { id: snippet.id, name: nextName }); await loadSnippets(); }
      catch (error) { await showAlert(errorMessage(error, t, 'snippets.renameFailed'), t('snippets.notRenamed')); }
    });
    const remove = document.createElement('button'); remove.className = 'button small danger'; remove.type = 'button'; remove.textContent = t('common.delete');
    remove.addEventListener('click', async (event) => {
      event.stopPropagation();
      if (!await showConfirm(t('snippets.deleteConfirm', { name: snippet.name }), t('snippets.deleteTitle'), true)) return;
      try { await invoke('delete_snippet', { id: snippet.id }); await loadSnippets(); }
      catch (error) { await showAlert(errorMessage(error, t, 'snippets.deleteFailed'), t('snippets.notDeleted')); }
    });
    actions.append(rename, remove);
    item.append(name, sql, meta, actions); item.addEventListener('click', () => { setEditorValue(snippet.sql); switchView('query'); showQueryWorkspaceSection('editor'); }); list.append(item);
  }
}

function showQueryWorkspaceSection(section) {
  for (const button of document.querySelectorAll('[data-query-workspace]')) {
    const active = button.dataset.queryWorkspace === section;
    button.classList.toggle('active', active);
    button.setAttribute('aria-selected', String(active));
  }
  for (const panel of document.querySelectorAll('[data-query-workspace-panel]')) panel.hidden = panel.dataset.queryWorkspacePanel !== section;
  if (section === 'history') void loadHistory();
  if (section === 'snippets') void loadSnippets();
}

function showAdminWorkspaceSection(section) {
  for (const button of document.querySelectorAll('[data-admin-workspace]')) {
    const active = button.dataset.adminWorkspace === section;
    button.classList.toggle('active', active);
    button.setAttribute('aria-selected', String(active));
  }
  for (const panel of document.querySelectorAll('[data-admin-workspace-panel]')) panel.hidden = panel.dataset.adminWorkspacePanel !== section;
  if (section === 'backup' && !byId('backup-connection').value && byId('admin-connection').value) {
    byId('backup-connection').value = byId('admin-connection').value; syncConnectionMarkers();
    const connection = state.connections.find((item) => item.id === byId('backup-connection').value);
    if (connection) byId('restore-database').value = connection.database;
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
      const row = item.closest('.tree-object-row, .tree-table-row');
      if (row) row.hidden = !matches; else item.hidden = !matches;
      tableMatches ||= matches;
    }
    for (const folder of children?.querySelectorAll('.tree-folder') || []) {
      const hasMatch = [...folder.querySelectorAll('.tree-item')].some((item) => !item.closest('.tree-object-row')?.hidden);
      folder.hidden = Boolean(query) && !hasMatch;
      if (query && hasMatch) {
        folder.querySelector('.tree-folder-children').hidden = false;
        folder.querySelector('.tree-folder-toggle').setAttribute('aria-expanded', 'true');
        folder.querySelector('.tree-folder-marker').textContent = '▾';
      }
    }
    const loadFailed = group.dataset.loadState === 'error';
    group.hidden = Boolean(query) && !schemaMatches && !tableMatches && !loadFailed;
  }
}

function updateExplorerObjectActions() {
  const connected = state.connections.some((connection) => connection.id === state.selectedConnectionId && connection.state === 'connected');
  const hasSchema = connected && Boolean(state.selectedSchema);
  const writable = !isReadOnly(state.selectedConnectionId);
  byId('new-schema').disabled = !connected || !writable;
  for (const id of ['new-table', 'new-function', 'new-procedure', 'new-sequence', 'new-trigger']) byId(id).disabled = !hasSchema || !writable;
}

function formatEstimatedRows(value) {
  return t('explorer.estimatedRows', { rows: new Intl.NumberFormat(t.locale, { notation: 'compact', maximumFractionDigits: 1 }).format(Math.max(0, Number(value) || 0)) });
}

function explorerSection(children, label) {
  const heading = document.createElement('div');
  heading.className = 'tree-section-label';
  heading.textContent = label;
  children.append(heading);
}

function explorerFolder(children, label) {
  const folder = document.createElement('section'); folder.className = 'tree-folder';
  const toggle = document.createElement('button'); toggle.type = 'button'; toggle.className = 'tree-folder-toggle'; toggle.setAttribute('aria-expanded', 'true');
  const labelNode = document.createElement('span'); labelNode.textContent = label;
  const marker = document.createElement('span'); marker.className = 'tree-folder-marker'; marker.textContent = '▾';
  toggle.append(marker, labelNode);
  const content = document.createElement('div'); content.className = 'tree-folder-children';
  toggle.addEventListener('click', () => {
    content.hidden = !content.hidden;
    toggle.setAttribute('aria-expanded', String(!content.hidden));
    marker.textContent = content.hidden ? '▸' : '▾';
  });
  folder.append(toggle, content); children.append(folder); return content;
}

function appendSchemaObjectRow(parent, id, schemaName, object) {
  const icons = { function: 'ƒ', procedure: '⚙', sequence: '↗', trigger: '⚡' };
  const item = document.createElement('button'); item.type = 'button'; item.className = 'tree-item schema-object';
  const name = document.createElement('span'); name.className = 'tree-item-label'; name.textContent = `${icons[object.kind]} ${object.name}`;
  const detail = document.createElement('small'); detail.textContent = object.detail || object.kind;
  item.append(name, detail); item.addEventListener('click', () => openSchemaObject(id, schemaName, object));
  const row = document.createElement('div'); row.className = 'tree-object-row';
  if (object.kind === 'sequence') {
    const next = document.createElement('button'); next.className = 'button small tree-object-action'; next.type = 'button'; next.textContent = t('explorer.sequenceNext');
    next.addEventListener('click', () => void advanceSequence(id, schemaName, object.name));
    const reset = document.createElement('button'); reset.className = 'button small danger tree-object-action'; reset.type = 'button'; reset.textContent = t('explorer.sequenceReset');
    reset.addEventListener('click', () => void resetSequence(id, schemaName, object.name));
    const edit = document.createElement('button'); edit.className = 'button small tree-object-action'; edit.type = 'button'; edit.textContent = t('common.edit');
    edit.addEventListener('click', () => editSequenceDefinition(id, schemaName, object));
    row.append(item, next, reset, edit);
  } else {
    const edit = document.createElement('button'); edit.className = 'button small tree-object-action'; edit.type = 'button'; edit.textContent = t('common.edit');
    edit.addEventListener('click', () => {
      if (object.kind === 'trigger') void openProgrammingFromExplorer(id, schemaName, object);
      else void openProgrammingFromExplorer(id, schemaName, object);
    });
    row.append(item, edit);
    if (['function', 'procedure', 'trigger'].includes(object.kind)) {
      const del = document.createElement('button'); del.className = 'button small danger tree-object-action'; del.type = 'button'; del.textContent = t('common.delete');
      del.disabled = Boolean(object.is_extension);
      del.title = object.is_extension ? t('explorer.extensionObject') : t(`explorer.deleteKind.${object.kind}`);
      del.addEventListener('click', () => void deleteSchemaProgrammingObject(id, schemaName, object, () => row.remove()));
      row.append(del);
    }
  }
  parent.append(row);
}

function openAdminForConnection(id) {
  state.selectedConnectionId = id;
  byId('admin-connection').value = id; syncConnectionMarkers();
  switchView('admin');
  showAdminWorkspaceSection('administration');
}

function openSchemaObject(id, schema, object) {
  state.selectedConnectionId = id;
  state.selectedSchema = schema;
  if (object.kind === 'trigger' && object.parent_table) {
    openTable(id, schema, object.parent_table);
    return;
  }
  const sql = schemaObjectSql(schema, object);
  if (sql) {
    openSqlInNewTab(sql, id, object.name);
    return;
  }
  void showAlert(t('explorer.objectUnavailableMessage'), t('explorer.objectUnavailable'));
}

async function advanceSequence(id, schema, name) {
  if (!await showConfirm(t('sequence.advanceConfirm', { schema, name }), t('sequence.advanceTitle'), false, t('sequence.advance'))) return;
  try {
    const value = await invoke('next_sequence_value', { id, schema, name });
    await showAlert(t('sequence.nextValue', { value }), t('sequence.title', { schema, name }));
  } catch (error) {
    await showAlert(errorMessage(error, t, 'sequence.advanceFailed'), t('sequence.notChanged'));
  }
}

async function resetSequence(id, schema, name) {
  const value = await showPrompt(t('sequence.resetMessage', { schema, name }), t('sequence.resetTitle'), t('sequence.valueLabel'), '1');
  if (value === null) return;
  if (!/^-?\d+$/.test(value)) { await showAlert(t('sequence.invalidValueMessage'), t('sequence.invalidValue')); return; }
  if (!await showConfirm(t('sequence.resetConfirm', { schema, name, value }), t('sequence.resetConfirmTitle'), true, t('explorer.sequenceReset'))) return;
  try {
    await invoke('set_sequence_value', { id, schema, name, value });
    await showAlert(t('sequence.resetDone', { value }), t('sequence.title', { schema, name }));
  } catch (error) {
    await showAlert(errorMessage(error, t, 'sequence.resetFailed'), t('sequence.notChanged'));
  }
}

function renderExplorerExtensions(id, tree, extensions) {
  const installed = extensions?.installed || [];
  if (!installed.length) return;
  const group = document.createElement('div');
  group.className = 'tree-group';
  group.dataset.loaded = 'true';
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'tree-item';
  button.textContent = `▾ ${t('explorer.extensions', { count: installed.length })}`;
  const children = document.createElement('div');
  children.className = 'tree-children';
  button.addEventListener('click', () => {
    children.hidden = !children.hidden;
    button.textContent = `${children.hidden ? '▸' : '▾'} ${t('explorer.extensions', { count: installed.length })}`;
  });
  for (const extension of installed) {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = 'tree-item schema-object';
    const name = document.createElement('span');
    name.className = 'tree-item-label';
    name.textContent = `⬡ ${extension.name}`;
    const detail = document.createElement('small');
    detail.textContent = extension.installed_version || 'installed';
    item.append(name, detail);
    item.addEventListener('click', () => openAdminForConnection(id));
    children.append(item);
  }
  group.append(button, children);
  tree.append(group);
}

function renderExplorerRetry(id, title, message) {
  const tree = byId('explorer-tree');
  const failure = errorState(title, message);
  const retry = document.createElement('button');
  retry.type = 'button';
  retry.className = 'button small';
  retry.textContent = t('common.retry');
  retry.addEventListener('click', () => void openExplorer(id));
  failure.append(retry);
  tree.replaceChildren(failure);
}

function renderExplorerTables(id, schemaName, children, tables, objects = []) {
  children.replaceChildren();
  if (tables.length) explorerSection(children, t('explorer.tablesAndViews'));
  for (const table of tables) {
    const tableItem = document.createElement('button');
    tableItem.type = 'button';
    tableItem.className = 'tree-item';
    const tableName = document.createElement('span');
    tableName.className = 'tree-item-label';
    tableName.textContent = `${table.kind === 'view' ? '◇' : '▦'} ${table.name}`;
    const estimate = document.createElement('small');
    estimate.textContent = formatEstimatedRows(table.estimated_rows);
    tableItem.append(tableName, estimate);
    tableItem.addEventListener('click', () => openTable(id, schemaName, table.name, table.kind));
    const row = document.createElement('div'); row.className = 'tree-table-row';
    const edit = document.createElement('button'); edit.className = 'button small tree-object-action'; edit.type = 'button'; edit.textContent = t('common.edit');
    if (table.kind === 'table') {
      edit.addEventListener('click', async () => {
        edit.disabled = true;
        try {
          const payload = await invoke('table_detail', { id, schema: schemaName, table: table.name });
          openAlterTableDialog(id, schemaName, table.name, payload.detail, {
            preserveExplorer: true,
            onApplied: (newName) => { table.name = newName; tableName.textContent = `▦ ${newName}`; },
          });
        } catch (error) {
          await showAlert(errorMessage(error, t, 'explorer.tableStructureFailed'), t('explorer.tableEditorUnavailable'));
        } finally {
          edit.disabled = false;
        }
      });
    } else {
      edit.addEventListener('click', () => void editViewDefinition(id, schemaName, table.name));
    }
    row.append(tableItem, edit); children.append(row);
  }
  const { programming, sequences } = groupSchemaObjects(objects);
  const programmingArea = explorerFolder(children, t('nav.programming'));
  if (programming.length) {
    for (const object of programming) appendSchemaObjectRow(programmingArea, id, schemaName, object);
  } else {
    const empty = document.createElement('div'); empty.className = 'tree-folder-empty'; empty.textContent = t('explorer.noProgramming'); programmingArea.append(empty);
  }
  if (sequences.length) explorerSection(children, t('explorer.sequences'));
  for (const object of sequences) appendSchemaObjectRow(children, id, schemaName, object);
}

function cancelExplorerSchemaLoad(group, schemaName) {
  group.dataset.requestToken = String(Number(group.dataset.requestToken || 0) + 1);
  group.dataset.loadState = 'idle';
  group.dataset.loaded = 'false';
  const button = group.firstElementChild;
  const children = group.querySelector('.tree-children');
  if (button) button.textContent = `▸ ${schemaName}`;
  if (children) {
    children.hidden = true;
    children.replaceChildren();
  }
}

async function loadExplorerSchemaGroup(id, schemaName, group) {
  const token = Number(group.dataset.requestToken || 0) + 1;
  group.dataset.requestToken = String(token);
  group.dataset.loadState = 'loading';
  group.dataset.loaded = 'false';
  const button = group.firstElementChild;
  const children = group.querySelector('.tree-children');
  button.textContent = `× ${schemaName} · ${t('explorer.schemaLoading')}`;
  children.hidden = false;
  children.replaceChildren(errorState(t('explorer.loadingObjects'), t('explorer.loadingObjectsHint')));
  try {
    const [tables, objects] = await Promise.all([
      invoke('list_tables', { id, schema: schemaName }),
      invoke('list_schema_objects', { id, schema: schemaName }),
    ]);
    if (Number(group.dataset.requestToken) !== token || state.selectedConnectionId !== id || !group.isConnected) return false;
    renderExplorerTables(id, schemaName, children, tables, objects);
    group.dataset.loadState = 'loaded';
    group.dataset.loaded = 'true';
    button.textContent = `▾ ${schemaName}`;
    return true;
  } catch (error) {
    if (Number(group.dataset.requestToken) !== token || state.selectedConnectionId !== id || !group.isConnected) return false;
    group.dataset.loadState = 'error';
    group.dataset.loaded = 'false';
    button.textContent = `↻ ${schemaName}`;
    children.replaceChildren(errorState(t('explorer.objectsFailed'), t('explorer.objectsFailedHint')));
    return false;
  }
}

async function loadExplorerTablesForFilter(id) {
  const request = ++state.explorerFilterRequest;
  const groups = [...byId('explorer-tree').querySelectorAll('.tree-group')];
  await Promise.all(groups.filter((group) => group.dataset.schema && !['loaded', 'loading'].includes(group.dataset.loadState)).map(async (group) => {
    const schemaName = group.dataset.schema;
    await loadExplorerSchemaGroup(id, schemaName, group);
  }));
  if (request === state.explorerFilterRequest) filterExplorerTree();
}

async function openExplorer(id) {
  const request = ++state.explorerConnectionRequest;
  ++state.explorerFilterRequest;
  state.selectedConnectionId = id;
  state.selectedSchema = null;
  updateExplorerObjectActions();
  state.explorerFilter = '';
  byId('explorer-filter').value = '';
  byId('open-erd').disabled = true;
  const connection = state.connections.find((item) => item.id === id);
  if (!connection) return;
  if (connection.state !== 'connected') await connect(id);
  if (request !== state.explorerConnectionRequest || state.selectedConnectionId !== id) return;
  renderExplorerConnections();
  const current = state.connections.find((item) => item.id === id);
  if (current?.state !== 'connected') {
    byId('explorer-status').textContent = t('explorer.connectFailed');
    renderExplorerRetry(id, t('explorer.connectFailedTitle'), t('explorer.connectFailedHint'));
    return;
  }
  byId('explorer-status').textContent = current.label;
  updateExplorerObjectActions();
  const tree = byId('explorer-tree');
  tree.replaceChildren();
  const loading = document.createElement('div');
  loading.className = 'empty-state';
  loading.textContent = t('explorer.loadingSchemas');
  tree.append(loading);
  try {
    const [schemasResult, extensionsResult] = await Promise.allSettled([
      invoke('list_schemas', { id }),
      invoke('list_extensions', { id }),
    ]);
    if (request !== state.explorerConnectionRequest || state.selectedConnectionId !== id) return;
    if (schemasResult.status === 'rejected') throw schemasResult.reason;
    const schemas = schemasResult.value;
    tree.replaceChildren();
    if (extensionsResult.status === 'fulfilled') renderExplorerExtensions(id, tree, extensionsResult.value);
    for (const schema of schemas) {
      const group = document.createElement('div');
      group.className = 'tree-group';
      group.dataset.schema = schema.name;
      group.dataset.loaded = 'false';
      group.dataset.loadState = 'idle';
      group.dataset.requestToken = '0';
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'tree-item';
      button.textContent = `▸ ${schema.name}`;
      const children = document.createElement('div');
      children.className = 'tree-children';
      button.addEventListener('click', async () => {
        state.selectedSchema = schema.name;
        byId('open-erd').disabled = false;
        updateExplorerObjectActions();
        if (group.dataset.loadState === 'loading') {
          cancelExplorerSchemaLoad(group, schema.name);
          return;
        }
        if (group.dataset.loadState === 'loaded') {
          children.hidden = !children.hidden;
          button.textContent = `${children.hidden ? '▸' : '▾'} ${schema.name}`;
          return;
        }
        await loadExplorerSchemaGroup(id, schema.name, group);
        filterExplorerTree();
      });
      group.append(button, children);
      tree.append(group);
      filterExplorerTree();
    }
    if (!schemas.length) tree.append(errorState(t('explorer.noSchemas'), t('explorer.noSchemasHint')));
  } catch (error) {
    if (request !== state.explorerConnectionRequest || state.selectedConnectionId !== id) return;
    renderExplorerRetry(id, t('explorer.schemasFailed'), t('explorer.schemasFailedHint'));
  }
}

function switchView(name) {
  for (const button of document.querySelectorAll('[data-view]')) {
    const active = button.dataset.view === name;
    button.classList.toggle('active', active);
    if (active) button.setAttribute('aria-current', 'page'); else button.removeAttribute('aria-current');
  }
  for (const view of ['connections', 'explorer', 'programming', 'dashboard', 'admin', 'assistant', 'query', 'preferences', 'table-detail', 'erd']) byId(`view-${view}`).hidden = view !== name;
  byId('page-title').textContent = t(`nav.${name}`);
  if (name === 'explorer') renderExplorerConnections();
  if (name === 'programming') openProgramming();
  if (name === 'dashboard') loadDashboard(byId('dashboard-connection').value);
  if (name === 'admin') loadAdmin(byId('admin-connection').value);
  if (name === 'assistant') loadAssistant(byId('assistant-connection').value);
}

const paletteCommands = [
  [t('connections.title'), t('palette.manageSavedPostgresqlConnections'), 'connections'],
  [t('nav.explorer'), t('palette.browseSchemasAndTables'), 'explorer'],
  [t('nav.programming'), t('palette.developViewsFunctionsProceduresAnd'), 'programming'],
  [t('nav.dashboard'), t('palette.inspectDatabaseHealthAndMetrics'), 'dashboard'],
  [t('nav.admin'), t('palette.reviewActivityLocksAndExtensions'), 'admin', null, 'administration'],
  [t('palette.backupRestore'), t('palette.protectOrRestorePostgresqlData'), 'admin', null, 'backup'],
  [t('nav.query'), t('palette.openAQueryWorkspace'), 'query', 'editor'],
  [t('history.title'), t('palette.reopenARecentQueryIn'), 'query', 'history'],
  [t('snippets.title'), t('palette.openReusableSqlSnippetsIn'), 'query', 'snippets'],
  [t('nav.preferences'), t('palette.themeColorsUpdatesAndAbout'), 'preferences'],
];

let commandSearchRequest = 0;

async function renderCommandPalette(filter = '') {
  const list = byId('command-list');
  list.replaceChildren();
  const query = filter.trim().toLowerCase();
  const commands = paletteCommands.filter(([name, description]) => `${name} ${description}`.toLowerCase().includes(query));
  if (!commands.length && query.length < 2) { list.append(errorState(t('palette.noCommandsFound'), t('palette.tryAnotherSearchTerm'))); return; }
  for (const [name, description, view, querySection, adminSection] of commands) {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = 'command-item';
    const title = document.createElement('strong'); title.textContent = name;
    const hint = document.createElement('small'); hint.textContent = description;
    item.append(title, hint);
    item.addEventListener('click', () => {
      closeCommandPalette();
      if (view === 'assistant') { assistantReturnView = null; byId('assistant-back').hidden = true; }
      switchView(view);
      if (querySection) showQueryWorkspaceSection(querySection);
      if (adminSection) showAdminWorkspaceSection(adminSection);
    });
    list.append(item);
  }
  if (query.length < 2 || !state.selectedConnectionId) {
    if (!commands.length) list.append(errorState(t('palette.searchTheDatabase'), t('palette.connectToPostgresqlAndType')));
    return;
  }
  const request = ++commandSearchRequest;
  const loading = errorState(t('palette.searchingDatabase'), t('palette.lookingThroughTablesViewsColumns'));
  list.append(loading);
  try {
    const results = await invoke('global_search', { id: state.selectedConnectionId, term: query });
    if (request !== commandSearchRequest || byId('command-palette').hidden) return;
    loading.remove();
    if (!results.length) { list.append(errorState(t('palette.noDatabaseObjectsFound'), t('palette.tryAnotherTermOrSchema'))); return; }
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
        if (result.table) openTable(state.selectedConnectionId, result.schema, result.table, result.kind === 'view' ? 'view' : 'table');
        else switchView('explorer');
      });
      list.append(item);
    }
  } catch (error) {
    if (request === commandSearchRequest) { loading.replaceChildren(); loading.append(errorState(t('palette.databaseSearchUnavailable'), t('palette.reconnectAndTryAgain'))); }
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

function showDialog({ title, message, eyebrow = 'DRACO', kind = 'confirm', confirmLabel = t('dialog.confirm'), cancelLabel = t('dialog.cancel'), inputLabel = t('dialog.value'), placeholder = '', inputValue = '' }) {
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

function showAlert(message, title = t('dialog.notice')) {
  return showDialog({ title, message, kind: 'alert', confirmLabel: t('dialog.ok') });
}

function showConfirm(message, title = t('dialog.confirmTitle'), danger = false, confirmLabel = danger ? t('dialog.delete') : t('dialog.confirm')) {
  return showDialog({ title, message, kind: danger ? 'confirm-danger' : 'confirm', confirmLabel });
}

function showPrompt(message, title = t('dialog.promptTitle'), inputLabel = t('dialog.value'), placeholder = '', inputValue = '') {
  return showDialog({ title, message, kind: 'prompt', confirmLabel: t('dialog.save'), inputLabel, placeholder, inputValue }).then((accepted) => accepted === null || accepted === false ? null : String(accepted).trim());
}

function showDangerPrompt(message, title, inputLabel, placeholder = '') {
  return showDialog({ title, message, kind: 'prompt-danger', confirmLabel: t('dialog.delete'), inputLabel, placeholder }).then((accepted) => accepted === null || accepted === false ? null : String(accepted).trim());
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
  button.title = maximized ? t('window.restore') : t('window.maximize');
  button.setAttribute('aria-label', button.title);
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
    toggle.setAttribute('aria-label', collapsed ? t('sidebar.show') : t('sidebar.hide'));
    toggle.title = collapsed ? t('sidebar.show') : t('sidebar.hide');
  });
}

async function boot() {
  const preferences = await loadPreferences();
  try {
    if (!invoke) throw new Error('Tauri IPC is unavailable');
    const health = await invoke('health');
    byId('health-label').textContent = health.ready ? t('health.ready') : t('health.unavailable');
    byId('health-value').textContent = health.ready ? t('health.bridgeOnline') : t('health.bridgeOffline');
    // A Flatpak store delivers updates itself, so the GitHub release check is not offered there.
    byId('updates-managed').hidden = !health.updates_managed;
    byId('check-updates-startup-row').hidden = Boolean(health.updates_managed);
    byId('update-card').hidden = Boolean(health.updates_managed);
    await refreshConnections();
    if (preferences.check_updates_on_startup && !health.updates_managed) void checkForUpdates(false);
  } catch (error) {
    byId('health-label').textContent = t('health.error');
    byId('health-value').textContent = t('health.bridgeUnavailable');
  }
}

byId('new-connection').addEventListener('click', () => showForm());
byId('open-command-palette').addEventListener('click', openCommandPalette);
for (const element of document.querySelectorAll('[data-close-command-palette]')) element.addEventListener('click', closeCommandPalette);
byId('command-search').addEventListener('input', (event) => renderCommandPalette(event.target.value));
document.addEventListener('keydown', (event) => {
  if ((event.ctrlKey || event.metaKey) && ['k', 'p'].includes(event.key.toLowerCase())) { event.preventDefault(); openCommandPalette(); }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 't') { event.preventDefault(); switchView('query'); showQueryWorkspaceSection('editor'); newQueryTab(); }
  if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === 's') { event.preventDefault(); switchView('query'); showQueryWorkspaceSection('editor'); void saveCurrentSnippet(); }
  if (event.key === 'F8') { event.preventDefault(); switchView('query'); showQueryWorkspaceSection('editor'); runQuery(); }
  if (event.key === 'F10') { event.preventDefault(); switchView('query'); showQueryWorkspaceSection('editor'); runQuery('explain'); }
  if (event.key === 'Escape' && !byId('ai-review-dialog').hidden) { event.preventDefault(); closeAiReviewDialog(); }
  if (event.key === 'Escape' && !byId('command-palette').hidden) closeCommandPalette();
});
byId('cancel-connection').addEventListener('click', hideForm);
byId('connection-form').addEventListener('submit', saveCurrentConnection);
byId('test-connection').addEventListener('click', testCurrentConnection);
byId('fill-from-url').addEventListener('click', fillFromConnectionUrl);
byId('connection-url').addEventListener('keydown', (event) => {
  if (event.key !== 'Enter') return;
  event.preventDefault();
  void fillFromConnectionUrl();
});
byId('import-connections').addEventListener('click', openConnectionImport);
byId('connection-import-reload').addEventListener('click', () => loadConnectionImport('preview_connection_imports'));
byId('connection-import-choose-service').addEventListener('click', () => loadConnectionImport('choose_connection_import_file', { source: 'service' }));
byId('connection-import-choose-pgpass').addEventListener('click', () => loadConnectionImport('choose_connection_import_file', { source: 'pgpass' }));
byId('connection-import-confirm').addEventListener('click', runConnectionImport);
byId('connection-import-cancel').addEventListener('click', closeConnectionImport);
for (const element of document.querySelectorAll('[data-close-connection-import]')) element.addEventListener('click', closeConnectionImport);
byId('connection-import-dialog').addEventListener('keydown', (event) => {
  if (event.key === 'Escape') void closeConnectionImport();
});
byId('table-import-choose').addEventListener('click', chooseTableImportFile);
byId('table-import-preview-button').addEventListener('click', previewTableImport);
byId('table-import-format').addEventListener('change', () => { syncTableImportControls(); void previewTableImport(); });
byId('table-import-run').addEventListener('click', runTableImport);
byId('table-import-cancel-run').addEventListener('click', cancelTableImport);
byId('table-import-close').addEventListener('click', closeTableImportDialog);
for (const element of document.querySelectorAll('[data-close-table-import]')) element.addEventListener('click', closeTableImportDialog);
byId('table-import-dialog').addEventListener('keydown', (event) => { if (event.key === 'Escape') closeTableImportDialog(); });
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
  byId('query-status').textContent = t('query.sqlFormatted');
});
byId('run-query').addEventListener('click', () => runQuery());
byId('run-script').addEventListener('click', () => runQuery('script'));
byId('explain-query').addEventListener('click', () => runQuery('explain'));
byId('cancel-query').addEventListener('click', cancelQuery);
byId('export-csv').addEventListener('click', () => downloadResult('csv'));
byId('export-json').addEventListener('click', () => downloadResult('json'));
byId('copy-result').addEventListener('click', copyResult);
byId('save-current-snippet').addEventListener('click', saveCurrentSnippet);
byId('review-query-ai').addEventListener('click', openAiReviewDialog);
byId('submit-ai-review').addEventListener('click', () => void submitAiReview());
byId('cancel-ai-review').addEventListener('click', closeAiReviewDialog);
for (const element of document.querySelectorAll('[data-close-ai-review]')) element.addEventListener('click', closeAiReviewDialog);
for (const button of document.querySelectorAll('[data-ai-review-focus]')) button.addEventListener('click', () => setAiReviewFocus(button.dataset.aiReviewFocus));
byId('clear-history').addEventListener('click', async () => { if (await showConfirm(t('history.clearConfirm'), t('history.clear'), true, t('history.clearAction'))) { await invoke('clear_history'); loadHistory(); } });
// Shared by the SQL editor and the Programming body editor: ArrowUp/Down/Enter/Tab/Escape
// steer the open suggestion popup instead of the editor when one is showing. Returns true if
// the key was consumed by the popup, so the caller's own shortcuts (Ctrl+Enter, Ctrl+S, ...)
// can skip acting on it.
function handleAutocompleteKeydown(event) {
  const popupOpen = Boolean(autocompletePopup && !autocompletePopup.hidden);
  if (popupOpen && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
    event.preventDefault();
    setActiveSuggestion((autocompleteActive + (event.key === 'ArrowDown' ? 1 : -1) + autocompleteItems.length) % autocompleteItems.length);
    return true;
  }
  if (popupOpen && (event.key === 'Enter' || event.key === 'Tab')) { event.preventDefault(); acceptSuggestion(); return true; }
  if (popupOpen && event.key === 'Escape') { event.preventDefault(); hideAutocomplete(); return true; }
  if (popupOpen && ['ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'].includes(event.key)) hideAutocomplete();
  return false;
}
byId('sql-editor').addEventListener('keydown', (event) => {
  if (handleAutocompleteKeydown(event)) return;
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
byId('programming-editor').addEventListener('input', () => {
  syncProgrammingEditorHighlight();
  notifyProgrammingFormChanged();
  void updateProgrammingAutocomplete();
});
byId('programming-save-file').addEventListener('click', () => void saveProgrammingFile());
byId('programming-choose-workspace').addEventListener('click', () => void chooseProgrammingWorkspace());
byId('programming-clear-workspace').addEventListener('click', () => void clearProgrammingWorkspace());
byId('programming-clear-workspace-local').addEventListener('click', () => void clearProgrammingWorkspace());
byId('choose-programming-workspace-preference').addEventListener('click', () => void chooseProgrammingWorkspace());
byId('clear-programming-workspace-preference').addEventListener('click', () => void clearProgrammingWorkspace());
byId('programming-run').addEventListener('click', openProgrammingRunDialog);
byId('programming-run-confirm').addEventListener('click', () => void runProgrammingDefinition());
byId('programming-run-cancel').addEventListener('click', closeProgrammingRunDialog);
for (const element of document.querySelectorAll('[data-close-programming-run]')) element.addEventListener('click', closeProgrammingRunDialog);
byId('programming-clear-output').addEventListener('click', () => { byId('programming-execution-output').hidden = true; byId('programming-execution-result').textContent = ''; });
byId('programming-refresh-source').addEventListener('click', () => { void loadProgrammingGithub(); void loadProgrammingLocalFiles(); });
byId('programming-repository').addEventListener('change', async (event) => {
  const [owner, repository] = event.target.value.split('/');
  if (!owner || !repository) return;
  try {
    githubConnection = await invoke('select_github_repository', { owner, repository });
    githubBranches = await invoke('github_branches');
    renderProgrammingGithubBranches();
    setProgrammingEditorStatus(t('programmingGit.repositorySelected', { repository: `${owner}/${repository}` }), 'success');
  } catch (error) {
    setProgrammingEditorStatus(errorMessage(error, t, 'programmingGit.couldNotSelectRepository'), 'error');
    renderProgrammingRepositoryWorkspace();
  }
});
byId('programming-workspace-branch').addEventListener('change', (event) => {
  const branch = event.target.value;
  if (programmingEditorTarget) { programmingEditorTarget.activeBranch = branch; byId('programming-github-branch').value = branch; }
});
byId('programming-editor').addEventListener('click', hideAutocomplete);
byId('programming-editor').addEventListener('scroll', () => {
  const overlay = byId('programming-editor-highlight');
  overlay.scrollTop = byId('programming-editor').scrollTop;
  overlay.scrollLeft = byId('programming-editor').scrollLeft;
  hideAutocomplete();
});
byId('programming-editor').addEventListener('keydown', (event) => {
  if (handleAutocompleteKeydown(event)) return;
  if (!(event.ctrlKey || event.metaKey)) return;
  if (event.key.toLowerCase() === 's') { event.preventDefault(); void saveProgrammingFile(); }
  if (event.key === 'Enter') { event.preventDefault(); void validateProgrammingDefinition(); }
});
byId('query-connection').addEventListener('change', () => { void completionIndexFor(byId('query-connection').value); });
document.addEventListener('click', (event) => {
  if (autocompletePopup && !autocompletePopup.hidden && !event.target.closest('.sql-editor-wrap')) hideAutocomplete();
});
byId('open-erd').addEventListener('click', openErd);
byId('back-to-explorer').addEventListener('click', returnToExplorer);
byId('new-schema').addEventListener('click', () => void createSchemaFromExplorer());
byId('new-table').addEventListener('click', () => openCreateTableDialog(state.selectedConnectionId, state.selectedSchema));
byId('new-function').addEventListener('click', () => void newProgrammingFromExplorer('function'));
byId('new-procedure').addEventListener('click', () => void newProgrammingFromExplorer('procedure'));
byId('new-sequence').addEventListener('click', () => void createSequenceFromExplorer());
byId('new-trigger').addEventListener('click', () => void newProgrammingFromExplorer('trigger'));
byId('explorer-filter').addEventListener('input', (event) => {
  state.explorerFilter = event.target.value;
  filterExplorerTree();
  if (state.explorerFilter.trim() && state.selectedConnectionId) void loadExplorerTablesForFilter(state.selectedConnectionId);
});
for (const id of CONNECTION_SELECTS) byId(id).addEventListener('change', syncConnectionMarkers);
byId('dashboard-connection').addEventListener('change', () => loadDashboard(byId('dashboard-connection').value));
byId('admin-connection').addEventListener('change', () => loadAdmin(byId('admin-connection').value));
byId('programming-connection').addEventListener('change', () => void loadProgrammingSchemas(byId('programming-connection').value));
byId('programming-schema').addEventListener('change', () => void loadProgrammingObjects(byId('programming-connection').value, byId('programming-schema').value));
byId('programming-new-function').addEventListener('click', () => void newProgrammingDefinition('function'));
byId('programming-new-procedure').addEventListener('click', () => void newProgrammingDefinition('procedure'));
byId('programming-new-trigger').addEventListener('click', () => void newProgrammingDefinition('trigger'));
byId('programming-new-view').addEventListener('click', () => void newProgrammingDefinition('view'));
byId('programming-back').addEventListener('click', () => void returnToProgrammingBrowser());
byId('programming-reload').addEventListener('click', () => void reloadProgrammingDefinition());
byId('programming-validate').addEventListener('click', () => void validateProgrammingDefinition());
byId('programming-review-ai').addEventListener('click', openProgrammingAiReviewDialog);
byId('programming-save').addEventListener('click', () => void saveProgrammingDefinition());
byId('pf-add-param').addEventListener('click', () => { byId('pf-params').append(functionParamRow()); notifyProgrammingFormChanged(); });
byId('pf-language').addEventListener('change', () => {
  const custom = byId('pf-language').value === '__custom__';
  byId('pf-language-custom').hidden = !custom;
  if (custom) byId('pf-language-custom').focus();
  const editor = byId('programming-editor');
  const body = editor.value.trim();
  if (byId('pf-language').value === 'sql' && /^BEGIN\b/i.test(body)) {
    setProgrammingEditorValue(SQL_FUNCTION_TEMPLATE_BODY);
    setProgrammingEditorStatus(t('programming.sqlBodyReplaced'), 'success');
  } else if (byId('pf-language').value === 'plpgsql' && /^SELECT\s+CURRENT_DATE\s*;?$/i.test(body)) {
    setProgrammingEditorValue(FUNCTION_TEMPLATE_BODY);
  }
  notifyProgrammingFormChanged();
});
byId('pf-returns-kind').addEventListener('change', () => {
  const kind = byId('pf-returns-kind').value;
  const current = classifyReturns(value('pf-returns'));
  setPfReturnsFromKind(kind, current.kind === kind ? current.detail : '');
  notifyProgrammingFormChanged();
  if (['scalar', 'setof', 'table'].includes(kind)) openPfReturnsDetailDialog();
});
byId('pf-returns-detail').addEventListener('click', () => openPfReturnsDetailDialog());
byId('programming-function-form').addEventListener('input', notifyProgrammingFormChanged);
byId('programming-function-form').addEventListener('change', notifyProgrammingFormChanged);
byId('programming-github-load').addEventListener('click', () => void loadProgrammingGithubFile());
byId('programming-github-diff-deployed').addEventListener('click', () => void diffProgrammingDeployed());
byId('programming-github-diff-branches').addEventListener('click', () => void diffProgrammingBranches());
byId('programming-github-commit').addEventListener('click', () => void commitProgrammingGithubFile());
byId('programming-github-push').addEventListener('click', () => void pushProgrammingGithubFile());
byId('programming-github-pr').addEventListener('click', openProgrammingPullRequestForm);
byId('programming-github-pr-cancel').addEventListener('click', () => { byId('programming-github-pr-form').hidden = true; });
byId('programming-github-pr-form').addEventListener('submit', (event) => void createProgrammingPullRequest(event));
byId('backup-connection').addEventListener('change', () => { const connection = state.connections.find((item) => item.id === byId('backup-connection').value); if (connection) byId('restore-database').value = connection.database; });
byId('assistant-connection').addEventListener('change', () => loadAssistant(byId('assistant-connection').value));
byId('run-backup').addEventListener('click', () => runBackup(false));
byId('run-restore').addEventListener('click', () => runBackup(true));
byId('choose-backup-output').addEventListener('click', () => void chooseBackupOutput());
byId('choose-restore-input').addEventListener('click', () => void chooseRestoreInput());
byId('backup-format').addEventListener('change', () => { byId('backup-output').value = ''; });
byId('cancel-operation').addEventListener('click', cancelOperation);
byId('send-assistant').addEventListener('click', sendAssistant);
byId('clear-assistant').addEventListener('click', async () => { const id = byId('assistant-connection').value; if (!id) return; await invoke('clear_assistant_history', { id }); loadAssistant(id); });
for (const button of document.querySelectorAll('[data-preference-section]')) button.addEventListener('click', () => showPreferenceSection(button.dataset.preferenceSection));
byId('ai-settings-form').addEventListener('submit', saveAssistantSettings);
byId('github-settings-form').addEventListener('submit', connectGithub);
byId('github-disconnect').addEventListener('click', () => void disconnectGithub());
byId('ai-provider').addEventListener('change', (event) => changeAssistantProvider(event.target.value));
byId('refresh-ai-models').addEventListener('click', () => void loadAssistantModels());
byId('save-ai-key').addEventListener('click', () => void saveAssistantKey());
byId('clear-ai-key').addEventListener('click', () => void clearAssistantKey());
for (const button of document.querySelectorAll('[data-query-workspace]')) button.addEventListener('click', () => showQueryWorkspaceSection(button.dataset.queryWorkspace));
for (const button of document.querySelectorAll('[data-admin-workspace]')) button.addEventListener('click', () => showAdminWorkspaceSection(button.dataset.adminWorkspace));
for (const button of document.querySelectorAll('[data-theme-choice]')) button.addEventListener('click', () => void savePreferences({ theme: button.dataset.themeChoice }));
for (const button of document.querySelectorAll('[data-accent-choice]')) button.addEventListener('click', () => void savePreferences({ accent: button.dataset.accentChoice }));
byId('check-updates-startup').addEventListener('change', (event) => void savePreferences({ check_updates_on_startup: event.target.checked }));
byId('notify-long-operations').addEventListener('change', (event) => void savePreferences({ notify_long_operations: event.target.checked }));
byId('check-updates').addEventListener('click', () => void checkForUpdates(true));
byId('copy-release-link').addEventListener('click', async () => { if (!state.releaseUrl) return; await navigator.clipboard.writeText(state.releaseUrl); byId('update-detail').textContent = t('updates.releaseLinkCopied'); });
byId('copy-pix-key').addEventListener('click', async () => { await navigator.clipboard.writeText(PIX_KEY); byId('pix-status').textContent = t('about.pixKeyCopied'); });
byId('copy-pix-code').addEventListener('click', async () => { await navigator.clipboard.writeText(PIX_COPY_AND_PASTE); byId('pix-status').textContent = t('about.pixCodeCopied'); });
for (const button of document.querySelectorAll('[data-view]')) button.addEventListener('click', () => {
  if (button.dataset.view === 'assistant') { assistantReturnView = null; byId('assistant-back').hidden = true; }
  switchView(button.dataset.view);
  if (button.dataset.view === 'query') showQueryWorkspaceSection('editor');
  if (button.dataset.view === 'admin') showAdminWorkspaceSection('administration');
});
byId('assistant-back').addEventListener('click', () => {
  const target = assistantReturnView;
  assistantReturnView = null;
  byId('assistant-back').hidden = true;
  if (target === 'admin') showAdminWorkspaceSection('administration');
  if (target) switchView(target);
});
bindWindowControls();
bindSidebarToggle();
bindAppDialog();
renderQueryTabs();
syncEditorHighlight();
applyAppearance(state.preferences);
boot();
