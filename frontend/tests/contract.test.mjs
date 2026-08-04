import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

const index = await readFile(new URL('../dist/index.html', import.meta.url), 'utf8');
const app = await readFile(new URL('../dist/app.js', import.meta.url), 'utf8');
const style = await readFile(new URL('../dist/styles.css', import.meta.url), 'utf8');
const pixQr = await readFile(new URL('../dist/pix-donation.svg', import.meta.url), 'utf8');

test('shell contains every registered application view', () => {
  for (const view of ['connections', 'explorer', 'dashboard', 'admin', 'assistant', 'query', 'preferences']) {
    assert.match(index, new RegExp(`id="view-${view}"`));
  }
  assert.doesNotMatch(index, /data-view="(?:history|snippets)"/);
  assert.doesNotMatch(index, /data-view="backup"/);
  assert.doesNotMatch(index, /id="view-backup"/);
  for (const section of ['administration', 'backup']) {
    assert.match(index, new RegExp(`data-admin-workspace="${section}"`));
    assert.match(index, new RegExp(`data-admin-workspace-panel="${section}"`));
  }
  assert.match(index, /id="admin-tabs"[^>]+role="tablist"/);
  assert.match(app, /function showAdminWorkspaceSection/);
  assert.match(app, /function showAdminPanel/);
  assert.match(app, /function renderAdminPanels/);
  for (const section of ['roles', 'jobs', 'extensions', 'query-stats', 'activity', 'locks']) assert.match(app, new RegExp(`\\['${section}',`));
  for (const section of ['editor', 'history', 'snippets']) {
    assert.match(index, new RegExp(`data-query-workspace="${section}"`));
    assert.match(index, new RegExp(`data-query-workspace-panel="${section}"`));
  }
  assert.match(app, /function showQueryWorkspaceSection/);
  assert.match(app, /if \(section === 'history'\) void loadHistory\(\)/);
  assert.match(app, /if \(section === 'snippets'\) void loadSnippets\(\)/);
  assert.match(index, /id="command-palette"[^>]+role="dialog"/);
  assert.match(index, /<script type="module" src="\.\/app\.js"><\/script>/);
  assert.match(index, /id="query-tabs"[^>]+role="tablist"/);
  for (const control of ['format-sql', 'review-query-ai', 'explain-query', 'copy-result']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(index, /id="sql-editor-highlight"[^>]+aria-hidden="true"/);
  assert.match(index, /id="sql-autocomplete"[^>]+role="listbox"/);
  for (const control of ['window-minimize', 'window-maximize', 'window-close']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(index, /data-tauri-drag-region/);
  assert.match(index, /id="sidebar-toggle"/);
  assert.match(index, /id="favorite"/);
  assert.match(index, /id="explorer-filter"/);
  for (const control of ['new-schema', 'new-table', 'new-function', 'new-sequence', 'new-trigger']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(index, /aria-expanded="true"/);
  assert.match(index, /id="app-dialog"[^>]+role="dialog"/);
  for (const control of ['app-dialog-confirm', 'app-dialog-cancel', 'app-dialog-input']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(index, /id="ai-review-dialog"[^>]+role="dialog"[^>]+aria-modal="true"/);
  for (const focus of ['general', 'performance', 'security', 'readability']) assert.match(index, new RegExp(`data-ai-review-focus="${focus}"`));
  for (const control of ['ai-review-query-preview', 'ai-review-note', 'submit-ai-review', 'cancel-ai-review']) assert.match(index, new RegExp(`id="${control}"`));
  for (const control of ['choose-backup-output', 'choose-restore-input']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(index, /id="backup-output"[^>]+readonly/);
  assert.match(index, /id="restore-input"[^>]+readonly/);
  assert.match(style, /\.app-dialog-input-wrap\[hidden\]\s*\{\s*display:\s*none;/);
  for (const section of ['appearance', 'ai', 'updates', 'about']) assert.match(index, new RegExp(`data-preference-section="${section}"`));
  for (const control of ['ai-provider', 'ai-model', 'ai-model-list', 'refresh-ai-models', 'ai-daily-limit', 'ai-round-limit', 'ai-api-key', 'save-ai-settings', 'save-ai-key', 'clear-ai-key']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(index, /id="ai-model"[^>]+list="ai-model-list"/);
  assert.match(index, /id="ai-api-key"[^>]+type="password"[^>]+autocomplete="new-password"/);
  assert.match(app, /function loadAssistantSettings/);
  assert.match(app, /function loadAssistantModels/);
  assert.match(app, /function saveAssistantSettings/);
  assert.match(app, /function saveAssistantKey/);
  assert.match(app, /function clearAssistantKey/);
  assert.match(app, /if \(name === 'ai'\) void loadAssistantSettings\(\)/);
  assert.match(index, /pix-donation\.svg/);
  assert.doesNotMatch(index, /britors@live\.com/);
  assert.match(pixQr, /viewBox="0 0 41 41"/);
});

test('frontend invokes only the typed local application bridge', () => {
  for (const command of ['health', 'preferences', 'save_preferences', 'check_for_updates', 'list_connections', 'list_schema_objects', 'next_sequence_value', 'set_sequence_value', 'create_schema', 'create_table', 'preview_alter_table', 'alter_table', 'create_sequence', 'create_trigger', 'function_definitions', 'validate_function_definition', 'save_function_definition', 'save_trigger_definition', 'completion_data', 'global_search', 'execute_query', 'execute_explain', 'rename_snippet', 'dashboard', 'browse_table_data', 'update_table_cell', 'insert_table_row', 'delete_table_row', 'cancel_activity', 'list_cron_jobs', 'create_cron_job', 'update_cron_job', 'cron_job_runs', 'set_cron_job_active', 'delete_cron_job', 'list_extensions', 'install_extension', 'drop_extension', 'query_stats', 'reset_query_stats', 'run_table_maintenance', 'list_roles', 'create_role', 'update_role', 'delete_role', 'choose_backup_output', 'choose_restore_input', 'run_backup', 'assistant_settings', 'save_assistant_settings', 'assistant_models', 'save_assistant_key', 'clear_assistant_key', 'assistant_send']) {
    assert.match(app, new RegExp(`['"]${command}['"]`));
  }
  assert.doesNotMatch(app, /localStorage|sessionStorage|fetch\(|XMLHttpRequest|dangerouslySetInnerHTML/);
  assert.doesNotMatch(app, /window\.(alert|confirm|prompt)\s*\(/);
  assert.doesNotMatch(index, /<script[^>]+src=["']https?:|<link[^>]+href=["']https?:/);
  assert.match(app, /import \{ resultRowToText, resultToTsv, serializeResult \} from '\.\/result-export\.js'/);
  assert.match(app, /import \{ highlightSqlIncremental \} from '\.\/sql-highlight\.js'/);
  assert.match(app, /import \{ AI_QUERY_REVIEW_FOCUSES, buildAiQueryReviewMessage \} from '\.\/ai-query-review\.js'/);
});

test('SQL editor ports the GTK AI review modal and sends only the current query context', () => {
  assert.match(app, /function openAiReviewDialog/);
  assert.match(app, /const sql = selectedQueryText\(\)\.trim\(\)/);
  assert.match(app, /aiReviewRequest = \{ connectionId, sql \}/);
  assert.match(app, /function setAiReviewFocus/);
  assert.match(app, /buildAiQueryReviewMessage\(aiReviewFocus, sql, byId\('ai-review-note'\)\.value\)/);
  assert.match(app, /byId\('assistant-connection'\)\.value = connectionId/);
  assert.match(app, /switchView\('assistant'\)/);
  assert.match(app, /await sendAssistant\(\)/);
  assert.match(app, /byId\('ai-review-query-preview'\)\.textContent = sql/);
  assert.doesNotMatch(app, /ai-review-query-preview'\)\.innerHTML/);
  assert.match(style, /\.ai-review-dialog\[hidden\]/);
  assert.match(style, /\.ai-review-focus-button\.active/);
});

test('backup and restore paths come only from native file pickers', () => {
  assert.match(app, /function chooseBackupOutput/);
  assert.match(app, /function chooseRestoreInput/);
  assert.match(app, /invoke\('choose_backup_output'/);
  assert.match(app, /invoke\('choose_restore_input'/);
  assert.match(style, /\.file-picker-row \{/);
});

test('Explorer exposes local object creators and validated definition editors', () => {
  assert.match(app, /function openCreateTableDialog/);
  assert.match(app, /function openAlterTableDialog/);
  assert.match(app, /function openTriggerCreator/);
  assert.match(app, /function definitionEditorDialog/);
  assert.match(app, /function editFunctionDefinition/);
  assert.match(app, /CREATE OR REPLACE TRIGGER/);
  assert.match(app, /result\.destructive/);
  assert.match(app, /References must use schema\.table\.column/);
  assert.match(style, /\.object-dialog \{/);
  assert.match(style, /\.column-editor-row \{/);
  assert.match(style, /\.definition-editor:focus/);
  assert.match(app, /className = 'tree-table-row'/);
  assert.match(app, /preserveExplorer: true/);
  assert.match(app, /closest\('\.tree-object-row, \.tree-table-row'\)/);
  assert.match(app, /options\.onApplied\?\.\(input\.new_table_name\)/);
  assert.match(app, /filterExplorerTree\(\);/);
  assert.match(style, /\.tree-table-row \{ display: grid; grid-template-columns: minmax\(0, 1fr\) auto;/);
});

test('table detail browses safely and edits rows only through complete primary keys', () => {
  assert.match(index, /id="back-to-explorer"/);
  assert.match(app, /function rememberExplorerState/);
  assert.match(app, /function returnToExplorer/);
  assert.match(app, /byId\('explorer-filter'\)\.value = state\.explorerFilter/);
  assert.match(app, /workspace\.scrollTop = explorerReturnState\.scrollTop/);
  assert.match(app, /byId\('back-to-explorer'\)\.addEventListener\('click', returnToExplorer\)/);
  assert.match(app, /function tableDataPanel/);
  assert.match(app, /limit: pageSize/);
  assert.match(app, /primary_key_columns/);
  assert.match(app, /read-only without a primary key/);
  assert.match(app, /value_json: row\[column\]/);
  assert.match(app, /values_json: valuesJson/);
  assert.match(app, /showConfirm\(`Permanently delete the row identified by/);
  assert.match(style, /\.table-data-grid \{/);
  assert.match(style, /\.table-cell-button:focus-visible/);
  assert.match(app, /function foreignKeyMapPanel/);
  assert.match(app, /openTable\(id, row\.foreign_schema, row\.foreign_table\)/);
  assert.match(style, /\.fk-map-row:focus-visible/);
});

test('the topbar stays fixed and only the workspace content scrolls, not the whole page', () => {
  assert.match(index, /<div class="workspace-content">/);
  assert.match(style, /body \{[^}]*height:\s*100vh;[^}]*overflow:\s*hidden/);
  assert.match(style, /\.app-shell \{[^}]*height:\s*100vh;[^}]*overflow:\s*hidden/);
  assert.match(style, /\.workspace \{[^}]*height:\s*100vh;[^}]*display:\s*flex;\s*flex-direction:\s*column;\s*overflow:\s*hidden/);
  assert.match(style, /\.topbar \{[^}]*flex:\s*0 0 auto/);
  assert.match(style, /\.workspace-content \{[^}]*flex:\s*1 1 auto;[^}]*overflow-y:\s*auto/);
  assert.match(style, /\.workspace-content \{[^}]*padding:\s*0 12px 72px 0/, 'a right-hand gutter keeps panel content from touching the scrollbar');
  const mobileBlock = style.match(/@media \(max-width: 860px\) \{[\s\S]*?\n\}/)[0];
  assert.match(mobileBlock, /body \{ height: auto; overflow: visible; \}/, 'below 860px the layout falls back to a single stacked page scroll');
  assert.match(mobileBlock, /\.workspace-content \{ padding-bottom: 0; overflow: visible; \}/);
});

test('query tabs can be renamed and closed, and always leave at least one tab open', () => {
  assert.match(app, /function renameQueryTab/);
  assert.match(app, /function closeQueryTab/);
  assert.match(app, /className = 'query-tab-close'/);
  assert.match(app, /showPrompt\('Choose a name for this query tab\.', 'Rename tab', 'Tab name', '', tab\.label\)/);
  assert.match(app, /if \(state\.queryTabs\.length === 1\) \{/);
  assert.match(style, /\.query-tab-wrap \{/);
  assert.match(style, /\.query-tab-close \{/);
});

test('connecting refreshes every connection dropdown, not just the connection list and Explorer', () => {
  const connectBody = app.match(/async function connect\(id\) \{[\s\S]*?\n\}/)[0];
  assert.match(connectBody, /renderQueryConnections\(\);/, 'Query connection select must refresh after connecting or the new session never appears there');
  assert.match(connectBody, /renderAdvancedConnections\(\);/, 'Dashboard/Admin/Backup/Assistant selects must refresh after connecting');
  assert.match(connectBody, /byId\('dashboard-connection'\)\.value = id;/, 'A successful connection should select its dashboard');
  assert.match(connectBody, /switchView\('dashboard'\);/, 'A successful connection should open its dashboard');
});

test('administration links a blocked session to its blocking activity', () => {
  assert.match(app, /function renderLocksPanel/);
  assert.match(app, /row\.dataset\.pid/);
  assert.match(app, /Show blocker/);
  assert.match(app, /activity\.scrollIntoView/);
  assert.match(style, /\.activity-item\.located/);
});

test('SQL editor overlays local syntax highlighting without introducing untrusted HTML', () => {
  assert.match(style, /\.sql-editor-wrap \{[^}]*display:\s*grid/);
  for (const token of ['keyword', 'type', 'string', 'number', 'comment', 'function', 'param']) {
    assert.match(style, new RegExp(`\\.sql-tok-${token}`));
  }
  assert.match(app, /function syncEditorHighlight/);
  assert.match(app, /function setEditorValue/);
  assert.match(app, /highlightSqlIncremental\(editor\.value, sqlHighlightCache\)/);
  assert.match(app, /overlay\.innerHTML = highlighted\.html/);
  assert.match(app, /syncEditorHighlight\(\); void updateAutocomplete\(\)/);
  assert.match(app, /byId\('sql-editor'\)\.addEventListener\('scroll'/);
  assert.doesNotMatch(app, /byId\('sql-editor'\)\.value = (?!text;)/, 'every sql-editor.value write must go through setEditorValue so the overlay never drifts out of sync');
});

test('the result grid renders only the rows in view instead of the whole dataset at once', () => {
  assert.match(app, /import \{ visibleRange \} from '\.\/virtual-list\.js'/);
  assert.match(app, /function renderVirtualizedRows/);
  assert.match(app, /function buildResultRow/);
  assert.match(app, /function buildResultHeader/);
  assert.match(app, /role', 'separator'/);
  assert.match(app, /\['ArrowLeft', 'ArrowRight'\]/);
  assert.match(app, /document\.createElement\('colgroup'\)/);
  assert.match(app, /grid\.addEventListener\('scroll', onScroll\)/);
  assert.match(app, /requestAnimationFrame\(\(\) => \{ frame = null; update\(\); \}\)/);
  assert.match(app, /resultVirtualCleanup = renderVirtualizedRows/);
  assert.match(app, /if \(resultVirtualCleanup\) \{ resultVirtualCleanup\(\); resultVirtualCleanup = null; \}/);
  assert.doesNotMatch(app, /result\.rows\.slice\(0, 500\)/, 'the grid must not silently drop rows past a fixed cutoff anymore');
  assert.match(style, /\.result-grid tr\.result-row-alt td/);
  assert.match(style, /\.result-grid tr\.result-spacer td/);
  assert.match(style, /\.result-column-resizer/);
});

test('SQL editor offers schema-aware autocomplete with keyboard and pointer selection', () => {
  assert.match(app, /import \{ applySuggestion, buildCompletionIndex, suggest \} from '\.\/sql-autocomplete\.js'/);
  assert.match(app, /invoke\('completion_data', \{ id \}\)/);
  assert.match(app, /function completionIndexFor/);
  assert.match(app, /function caretPixelPosition/);
  assert.match(app, /function acceptSuggestion/);
  assert.match(app, /event\.key === 'ArrowDown' \|\| event\.key === 'ArrowUp'/);
  assert.match(app, /event\.key === 'Enter' \|\| event\.key === 'Tab'/);
  assert.match(app, /option\.addEventListener\('mousedown'/);
  assert.match(style, /\.sql-autocomplete \{/);
  assert.match(style, /\.sql-suggestion\.active/);
});

test('dark visual system is local and has explicit loading/error/empty surfaces', () => {
  assert.match(style, /background:\s*radial-gradient/);
  for (const token of ['--color-canvas', '--color-surface', '--color-action', '--focus-ring']) assert.match(style, new RegExp(token));
  assert.match(style, /:focus-visible/);
  assert.match(style, /@media \(max-width: 860px\)/);
  assert.match(style, /@media \(max-width: 720px\)/);
  assert.match(style, /\.connection-card \{ grid-template-columns: 1fr; \}/);
  assert.match(style, /\.query-toolbar label \{ min-width: 0; \}/);
  assert.match(style, /\.button:disabled/);
  assert.match(style, /html\[data-theme="light"\]/);
  assert.match(app, /function applyAppearance/);
  assert.match(app, /function checkForUpdates/);
  assert.match(app, /check_updates_on_startup/);
  assert.match(app, /PIX_COPY_AND_PASTE/);
  assert.match(index, /empty-state/);
  assert.match(index, /role="status"/);
  assert.match(app, /Loading dashboard/);
  assert.match(app, /openCommandPalette/);
  assert.match(app, /function bindSidebarToggle/);
  assert.match(app, /sidebar\.classList\.toggle\('collapsed'\)/);
  assert.match(app, /favorite: byId\('favorite'\)\.checked/);
  assert.match(app, /sort\(\(left, right\) => Number\(right\.favorite\) - Number\(left\.favorite\)\)/);
  assert.match(app, /function filterExplorerTree/);
  assert.match(app, /function loadExplorerTablesForFilter/);
  assert.match(app, /function loadExplorerSchemaGroup/);
  assert.match(app, /function cancelExplorerSchemaLoad/);
  assert.match(app, /function renderExplorerRetry/);
  assert.match(app, /retry\.addEventListener\('click', \(\) => void openExplorer\(id\)\)/);
  assert.match(app, /group\.dataset\.loadState === 'loading'/);
  assert.match(app, /Click the schema to retry/);
  assert.match(app, /request !== state\.explorerConnectionRequest/);
  assert.match(app, /invoke\('list_tables', \{ id, schema: schemaName \}\)/);
  assert.match(app, /invoke\('list_schema_objects', \{ id, schema: schemaName \}\)/);
  assert.match(app, /Functions & procedures/);
  assert.match(app, /Sequences/);
  assert.match(app, /Triggers/);
  assert.match(app, /function openSchemaObject/);
  assert.match(app, /function advanceSequence/);
  assert.match(app, /function resetSequence/);
  assert.match(app, /Existing table values are not checked/);
  assert.match(app, /function renderExplorerExtensions/);
  assert.match(app, /openSqlInNewTab\(sql, id, object\.name\)/);
  assert.match(app, /item\.addEventListener\('click', \(\) => openSchemaObject\(id, schemaName, object\)\)/);
  assert.match(app, /invoke\('list_extensions', \{ id \}\)/);
  assert.doesNotMatch(app, /:scope/);
  assert.match(app, /state\.explorerFilter/);
  assert.match(app, /formatEstimatedRows/);
  assert.match(app, /table\.estimated_rows/);
  assert.match(app, /Searching database/);
  assert.match(app, /newQueryTab/);
  assert.match(app, /const connectButton = document\.createElement\('button'\)/);
  assert.match(app, /connectButton\.className = `button small \$\{connection\.state === 'connected' \? 'disconnect-action' : 'connect-action'\}`/);
  assert.match(app, /connectButton\.addEventListener\('click', \(\) => connection\.state === 'connected' \? disconnect\(connection\.id\) : connect\(connection\.id\)\)/);
  assert.match(app, /selectedQueryText/);
  assert.match(app, /className = 'button small row-detail-action'/);
  assert.match(app, /showAlert\(resultRowToText\(row, result\.columns\)/);
  assert.match(app, /navigator\.clipboard\.writeText\(resultToTsv\(state\.result\)\)/);
  assert.match(app, /codePanel\('DDL', payload\.ddl\)/);
  assert.match(app, /payload\.column_stats/);
  assert.match(app, /function tableMaintenancePanel/);
  assert.match(app, /ACCESS EXCLUSIVE lock/);
  assert.match(app, /\['vacuum_full', 'Vacuum Full'\]/);
  assert.match(app, /formatSql/);
  assert.match(app, /event\.key === 'F8'/);
  assert.match(app, /event\.key === 'F10'/);
  assert.match(app, /\['k', 'p'\]\.includes\(event\.key\.toLowerCase\(\)\)/);
  assert.match(app, /event\.key === 'F10'[^\n]+runQuery\('explain'\)/);
  assert.doesNotMatch(app, /event\.key === 'F10'[^\n]+runQuery\('script'\)/);
  assert.match(app, /event\.key\.toLowerCase\(\) === 't'/);
  assert.match(app, /event\.key\.toLowerCase\(\) === 's'/);
  assert.match(app, /function saveCurrentSnippet/);
  assert.match(app, /Rename snippet/);
  assert.match(app, /Delete the snippet/);
  assert.match(app, /renderErdCanvas/);
  assert.match(app, /erd-node/);
  assert.match(app, /erd-relations-panel/);
  assert.match(style, /\.erd-canvas/);
  assert.match(style, /\.tree-item\[hidden\], \.tree-group\[hidden\] \{ display: none; \}/);
  assert.match(style, /\.sidebar\.collapsed/);
  assert.match(style, /\.sidebar-toggle \{ position: fixed; top: 28px; left: 233px; z-index: 20;/);
  assert.match(style, /\.sidebar\.collapsed \.sidebar-toggle \{ left: 61px; \}/);
  assert.match(style, /\.sidebar \{ position: relative;/);
  assert.match(style, /\.sidebar\.collapsed \.nav-item::after/);
  assert.match(style, /\.workspace \{ width: auto; max-width: none; flex: 1 1 auto;/);
  assert.match(style, /\.button\.connect-action/);
  assert.match(style, /\.button\.disconnect-action/);
  for (const command of ['startDragging', 'minimize', 'toggleMaximize']) assert.match(app, new RegExp(command));
  assert.match(app, /currentQueryOperationId/);
  assert.match(app, /operationId: state\.currentQueryOperationId/);
  assert.match(app, /Confirm restore/);
  assert.match(app, /'Confirm restore', true, 'Restore'/);
  assert.match(app, /function renderRolesPanel/);
  assert.match(app, /editingRole = role\.name/);
  assert.match(app, /Save changes/);
  assert.match(app, /valid_until: expiration\.value \|\| null/);
  assert.match(app, /function renderActivityPanel/);
  assert.match(app, /function renderCronJobsPanel/);
  assert.match(app, /editingJob = job/);
  assert.match(app, /cron_job_runs/);
  assert.match(app, /No executions were recorded for this job/);
  assert.match(app, /function renderExtensionsPanel/);
  assert.match(app, /Promise\.allSettled/);
  assert.match(app, /Scheduled jobs unavailable/);
  assert.match(app, /Extensions unavailable/);
  assert.match(app, /built-in plpgsql extension is protected/);
  assert.match(app, /Extension installation executes SQL/);
  assert.match(app, /function renderQueryStatsPanel/);
  assert.match(app, /function askAssistantAboutQueryStat/);
  assert.match(app, /Ask assistant/);
  assert.match(app, /pg_stat_statements recorded/);
  assert.match(app, /Reset all pg_stat_statements counters/);
  assert.match(app, /openSqlInNewTab\(queryStat\.query, id\)/);
  assert.match(app, /\['calls', 'Calls'\]/);
  assert.match(app, /\['mean', 'Mean time'\]/);
  assert.match(app, /pg_cron is not installed/);
  assert.match(app, /Permanently delete the scheduled job/);
  assert.match(app, /Only active queries can be cancelled/);
  assert.match(app, /'Cancel active query', true, 'Cancel query'/);
  assert.match(app, /function showDangerPrompt/);
  assert.match(app, /function showPrompt[\s\S]{0,300}String\(accepted\)\.trim\(\)/);
  assert.match(app, /confirmation !== role\.name/);
  assert.match(app, /role\.name\.toLowerCase\(\)\.startsWith\('pg_'\)/);
  assert.match(style, /\.roles-panel \{ grid-column: 1 \/ -1;/);
  assert.match(style, /\.ddl-code/);
  assert.match(app, /run-backup', 'run-restore'/);
  for (const helper of ['showAlert', 'showConfirm', 'showPrompt']) assert.match(app, new RegExp(`function ${helper}`));
});
