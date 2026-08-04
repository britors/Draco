import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

const index = await readFile(new URL('../dist/index.html', import.meta.url), 'utf8');
const app = await readFile(new URL('../dist/app.js', import.meta.url), 'utf8');
const style = await readFile(new URL('../dist/styles.css', import.meta.url), 'utf8');
const pixQr = await readFile(new URL('../dist/pix-donation.svg', import.meta.url), 'utf8');

test('shell contains every registered application view', () => {
  for (const view of ['connections', 'explorer', 'dashboard', 'admin', 'backup', 'assistant', 'query', 'history', 'snippets', 'preferences']) {
    assert.match(index, new RegExp(`id="view-${view}"`));
  }
  assert.match(index, /id="command-palette"[^>]+role="dialog"/);
  assert.match(index, /<script type="module" src="\.\/app\.js"><\/script>/);
  assert.match(index, /id="query-tabs"[^>]+role="tablist"/);
  for (const control of ['format-sql', 'explain-query', 'copy-result']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(index, /id="sql-editor-highlight"[^>]+aria-hidden="true"/);
  assert.match(index, /id="sql-autocomplete"[^>]+role="listbox"/);
  for (const control of ['window-minimize', 'window-maximize', 'window-close']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(index, /data-tauri-drag-region/);
  assert.match(index, /id="sidebar-toggle"/);
  assert.match(index, /id="favorite"/);
  assert.match(index, /id="explorer-filter"/);
  assert.match(index, /aria-expanded="true"/);
  assert.match(index, /id="app-dialog"[^>]+role="dialog"/);
  for (const control of ['app-dialog-confirm', 'app-dialog-cancel', 'app-dialog-input']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(style, /\.app-dialog-input-wrap\[hidden\]\s*\{\s*display:\s*none;/);
  for (const section of ['appearance', 'updates', 'about']) assert.match(index, new RegExp(`data-preference-section="${section}"`));
  assert.match(index, /pix-donation\.svg/);
  assert.doesNotMatch(index, /britors@live\.com/);
  assert.match(pixQr, /viewBox="0 0 41 41"/);
});

test('frontend invokes only the typed local application bridge', () => {
  for (const command of ['health', 'preferences', 'save_preferences', 'check_for_updates', 'list_connections', 'list_schema_objects', 'completion_data', 'global_search', 'execute_query', 'execute_explain', 'rename_snippet', 'dashboard', 'cancel_activity', 'list_cron_jobs', 'set_cron_job_active', 'delete_cron_job', 'list_extensions', 'install_extension', 'drop_extension', 'query_stats', 'reset_query_stats', 'run_table_maintenance', 'list_roles', 'create_role', 'delete_role', 'run_backup', 'assistant_send']) {
    assert.match(app, new RegExp(`['"]${command}['"]`));
  }
  assert.doesNotMatch(app, /localStorage|sessionStorage|fetch\(|XMLHttpRequest|dangerouslySetInnerHTML/);
  assert.doesNotMatch(app, /window\.(alert|confirm|prompt)\s*\(/);
  assert.doesNotMatch(index, /<script[^>]+src=["']https?:|<link[^>]+href=["']https?:/);
  assert.match(app, /import \{ resultRowToText, resultToTsv, serializeResult \} from '\.\/result-export\.js'/);
  assert.match(app, /import \{ highlightSql \} from '\.\/sql-highlight\.js'/);
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
});

test('SQL editor overlays local syntax highlighting without introducing untrusted HTML', () => {
  assert.match(style, /\.sql-editor-wrap \{[^}]*display:\s*grid/);
  for (const token of ['keyword', 'type', 'string', 'number', 'comment', 'function', 'param']) {
    assert.match(style, new RegExp(`\\.sql-tok-${token}`));
  }
  assert.match(app, /function syncEditorHighlight/);
  assert.match(app, /function setEditorValue/);
  assert.match(app, /overlay\.innerHTML = highlightSql\(editor\.value\)/);
  assert.match(app, /syncEditorHighlight\(\); void updateAutocomplete\(\)/);
  assert.match(app, /byId\('sql-editor'\)\.addEventListener\('scroll'/);
  assert.doesNotMatch(app, /byId\('sql-editor'\)\.value = (?!text;)/, 'every sql-editor.value write must go through setEditorValue so the overlay never drifts out of sync');
});

test('the result grid renders only the rows in view instead of the whole dataset at once', () => {
  assert.match(app, /import \{ visibleRange \} from '\.\/virtual-list\.js'/);
  assert.match(app, /function renderVirtualizedRows/);
  assert.match(app, /function buildResultRow/);
  assert.match(app, /grid\.addEventListener\('scroll', onScroll\)/);
  assert.match(app, /requestAnimationFrame\(\(\) => \{ frame = null; update\(\); \}\)/);
  assert.match(app, /resultVirtualCleanup = renderVirtualizedRows/);
  assert.match(app, /if \(resultVirtualCleanup\) \{ resultVirtualCleanup\(\); resultVirtualCleanup = null; \}/);
  assert.doesNotMatch(app, /result\.rows\.slice\(0, 500\)/, 'the grid must not silently drop rows past a fixed cutoff anymore');
  assert.match(style, /\.result-grid tr\.result-row-alt td/);
  assert.match(style, /\.result-grid tr\.result-spacer td/);
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
  assert.match(app, /invoke\('list_tables', \{ id, schema: schemaName \}\)/);
  assert.match(app, /invoke\('list_schema_objects', \{ id, schema: schemaName \}\)/);
  assert.match(app, /Functions & procedures/);
  assert.match(app, /Sequences/);
  assert.match(app, /Triggers/);
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
  assert.match(app, /function renderActivityPanel/);
  assert.match(app, /function renderCronJobsPanel/);
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
