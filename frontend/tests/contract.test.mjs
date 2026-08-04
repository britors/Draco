import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

const index = await readFile(new URL('../dist/index.html', import.meta.url), 'utf8');
const app = await readFile(new URL('../dist/app.js', import.meta.url), 'utf8');
const style = await readFile(new URL('../dist/styles.css', import.meta.url), 'utf8');

test('shell contains every registered application view', () => {
  for (const view of ['connections', 'explorer', 'dashboard', 'admin', 'backup', 'assistant', 'query', 'history', 'snippets']) {
    assert.match(index, new RegExp(`id="view-${view}"`));
  }
  assert.match(index, /id="command-palette"[^>]+role="dialog"/);
  assert.match(index, /id="query-tabs"[^>]+role="tablist"/);
  assert.match(index, /id="format-sql"/);
  for (const control of ['window-minimize', 'window-maximize', 'window-close']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(index, /data-tauri-drag-region/);
  assert.match(index, /id="sidebar-toggle"/);
  assert.match(index, /id="favorite"/);
  assert.match(index, /id="explorer-filter"/);
  assert.match(index, /aria-expanded="true"/);
  assert.match(index, /id="app-dialog"[^>]+role="dialog"/);
  for (const control of ['app-dialog-confirm', 'app-dialog-cancel', 'app-dialog-input']) assert.match(index, new RegExp(`id="${control}"`));
  assert.match(style, /\.app-dialog-input-wrap\[hidden\]\s*\{\s*display:\s*none;/);
});

test('frontend invokes only the typed local application bridge', () => {
  for (const command of ['health', 'list_connections', 'global_search', 'execute_query', 'dashboard', 'run_backup', 'assistant_send']) {
    assert.match(app, new RegExp(`['"]${command}['"]`));
  }
  assert.doesNotMatch(app, /localStorage|sessionStorage|fetch\(|XMLHttpRequest|dangerouslySetInnerHTML/);
  assert.doesNotMatch(app, /window\.(alert|confirm|prompt)\s*\(/);
  assert.doesNotMatch(index, /<script[^>]+src=["']https?:|<link[^>]+href=["']https?:/);
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
  assert.match(app, /formatSql/);
  assert.match(app, /event\.key === 'F8'/);
  assert.match(app, /event\.key === 'F10'/);
  assert.match(app, /event\.key\.toLowerCase\(\) === 't'/);
  assert.match(app, /event\.key\.toLowerCase\(\) === 's'/);
  assert.match(app, /function saveCurrentSnippet/);
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
  assert.match(app, /run-backup', 'run-restore'/);
  for (const helper of ['showAlert', 'showConfirm', 'showPrompt']) assert.match(app, new RegExp(`function ${helper}`));
});
