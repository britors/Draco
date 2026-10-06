// End-to-end smoke of the installed Draco package against a real PostgreSQL server.
//
// Run through `scripts/test-installed-app.sh`, which starts `tauri-driver`, points the app at a
// temporary XDG config containing one connection, and keeps the password in the credential store.
import test from 'node:test';
import assert from 'node:assert/strict';
import { Session } from './webdriver.mjs';

const CONNECTION_LABEL = process.env.DRACO_E2E_CONNECTION_LABEL || 'Draco E2E';

// Unique per run, so the fixture never collides with application data or an interrupted run.
const FIXTURE_SCHEMA = `draco_e2e_${Date.now()}`;
const FIXTURE_TABLE = 'e2e_items';

// Runs SQL from the editor (`run-query` or `run-script`) and resolves once the app has rendered
// the outcome: `{ rows }` from the raw row count, or `{ error }` with the visible message.
async function runSql(session, button, sql) {
  await session.click('[data-view="query"]');
  await session.click('[data-query-workspace="editor"]');
  await session.type('#sql-editor', sql);
  await session.run(`
    document.getElementById('result-summary').removeAttribute('data-rows');
    document.getElementById('result-error').textContent = '';
  `);
  await session.click(`#${button}`);
  return session.waitFor(`the outcome of ${button}`, `
    if (document.getElementById('run-query').disabled) return null;
    const error = document.getElementById('result-error').textContent.trim();
    if (error) return { error };
    const rows = document.getElementById('result-summary').dataset.rows;
    return rows === undefined ? null : { rows: Number(rows) };
  `);
}

// Returns the card of the E2E connection, located by its label rendered with textContent.
const FIND_CARD = `
  const [label] = arguments;
  return [...document.querySelectorAll('#connection-list .connection-card')]
    .find((card) => card.querySelector('strong')?.textContent === label) || null;
`;

test('installed app connects, browses the schema, runs queries and inspects a table', async (t) => {
  const session = await Session.start();
  let fixtureCreated = false;
  t.after(async () => {
    // Best effort: a failed cleanup must not hide the assertion that actually failed.
    if (fixtureCreated) {
      await runSql(session, 'run-script', `DROP SCHEMA IF EXISTS ${FIXTURE_SCHEMA} CASCADE`).catch(() => {});
    }
    await session.quit();
  });

  await t.test('lists the stored connection', async () => {
    await session.waitFor('the connection card', `return Boolean((function () { ${FIND_CARD} }).apply(null, arguments));`, CONNECTION_LABEL);
  });

  await t.test('connects using the password from the credential store', async () => {
    await session.run(`
      const card = (function () { ${FIND_CARD} }).apply(null, arguments);
      card.querySelector('.connect-action').click();
    `, CONNECTION_LABEL);
    const state = await session.waitFor('the connected state', `
      const card = (function () { ${FIND_CARD} }).apply(null, arguments);
      // data-state carries the raw state; the visible status text is translated.
      const status = card?.dataset.state || card?.querySelector('small')?.textContent;
      return status === 'connected' || status === 'error' ? status : null;
    `, CONNECTION_LABEL);
    assert.equal(state, 'connected');
  });

  await t.test('loads schemas in the Explorer', async () => {
    await session.click('[data-view="explorer"]');
    await session.waitFor('the Explorer connection', `
      const [label] = arguments;
      const item = [...document.querySelectorAll('#explorer-connections button')]
        .find((button) => button.textContent.includes(label));
      if (!item) return false;
      item.click();
      return true;
    `, CONNECTION_LABEL);
    const schemas = await session.waitFor('the schema tree', `
      const tree = document.getElementById('explorer-tree');
      const text = tree?.textContent || '';
      return text.includes('public') || text.includes('pg_catalog') ? text.length : null;
    `);
    assert.ok(schemas > 0);
  });

  await t.test('runs a query and renders the result grid', async () => {
    await session.click('[data-view="query"]');
    await session.waitFor('the connection selector', `
      const [label] = arguments;
      const select = document.getElementById('query-connection');
      const option = [...select.options].find((item) => item.textContent.includes(label));
      if (!option) return null;
      select.value = option.value;
      select.dispatchEvent(new Event('change', { bubbles: true }));
      return option.value;
    `, CONNECTION_LABEL);
    await session.type('#sql-editor', "SELECT 42 AS answer, 'draco' AS name");
    await session.click('#run-query');
    const summary = await session.waitFor('the query result', `
      const error = document.getElementById('result-error');
      if (error && !error.hidden && error.textContent.trim()) return 'error: ' + error.textContent.trim();
      // data-rows carries the raw row count; the visible summary is translated.
      const summary = document.getElementById('result-summary');
      const rows = summary.dataset.rows ?? summary.textContent.match(/^(\\d+) rows/)?.[1];
      return rows === undefined ? null : rows;
    `);
    assert.equal(summary, '1');
    const grid = await session.run(`return document.getElementById('result-grid').textContent;`);
    assert.match(grid, /answer/);
    assert.match(grid, /42/);
    assert.match(grid, /draco/);
  });

  await t.test('runs a script that creates an isolated fixture', async () => {
    fixtureCreated = true;
    const created = await runSql(session, 'run-script', [
      `CREATE SCHEMA ${FIXTURE_SCHEMA};`,
      `CREATE TABLE ${FIXTURE_SCHEMA}.${FIXTURE_TABLE} (id integer PRIMARY KEY, name text NOT NULL);`,
      `INSERT INTO ${FIXTURE_SCHEMA}.${FIXTURE_TABLE} VALUES (1, 'alpha'), (2, 'beta'), (3, 'gamma');`,
    ].join('\n'));
    assert.equal(created.error, undefined);
    const counted = await runSql(session, 'run-query', `SELECT count(*) AS total FROM ${FIXTURE_SCHEMA}.${FIXTURE_TABLE}`);
    assert.deepEqual(counted, { rows: 1 });
    assert.match(await session.run(`return document.getElementById('result-grid').textContent;`), /3/);
  });

  await t.test('reports a SQL error and recovers on the next query', async () => {
    const failed = await runSql(session, 'run-query', `SELECT * FROM ${FIXTURE_SCHEMA}.missing_table`);
    assert.ok(failed.error, 'expected the editor to show an error');
    const recovered = await runSql(session, 'run-query', 'SELECT 1 AS one');
    assert.deepEqual(recovered, { rows: 1 });
  });

  await t.test('opens the table detail from the Explorer', async () => {
    await session.click('[data-view="explorer"]');
    // Reopening the connection reloads the schema list, which now includes the fixture.
    await session.waitFor('the Explorer connection', `
      const [label] = arguments;
      const item = [...document.querySelectorAll('#explorer-connections button')]
        .find((button) => button.textContent === label);
      if (!item) return false;
      item.click();
      return true;
    `, CONNECTION_LABEL);
    await session.waitFor('the fixture schema', `
      const button = document.querySelector('.tree-group[data-schema="' + arguments[0] + '"] > .tree-item');
      if (!button) return false;
      button.click();
      return true;
    `, FIXTURE_SCHEMA);
    await session.waitFor('the fixture table', `
      const [schema, table] = arguments;
      const group = document.querySelector('.tree-group[data-schema="' + schema + '"]');
      const item = [...(group?.querySelectorAll('.tree-children .tree-item') || [])]
        .find((button) => button.querySelector('.tree-item-label')?.textContent.endsWith(' ' + table));
      if (!item) return false;
      item.click();
      return true;
    `, FIXTURE_SCHEMA, FIXTURE_TABLE);
    const detail = await session.waitFor('the table detail', `
      if (document.getElementById('view-table-detail').hidden) return null;
      const text = document.getElementById('detail-content').textContent;
      return text.includes('CREATE TABLE') ? text : null;
    `);
    assert.equal(await session.run(`return document.getElementById('detail-title').textContent;`), FIXTURE_TABLE);
    assert.match(detail, /integer · NOT NULL · PK/);
    assert.match(detail, /text · NOT NULL/);
  });

  await t.test('browses the table rows by primary key', async () => {
    const rows = await session.waitFor('the table data grid', `
      const cells = [...document.querySelectorAll('.table-data-grid tbody tr')]
        .map((row) => [...row.querySelectorAll('td')].slice(0, 2).map((cell) => cell.textContent.trim()));
      return cells.length ? cells : null;
    `);
    assert.deepEqual(rows, [['1', 'alpha'], ['2', 'beta'], ['3', 'gamma']]);
  });

  // The native file picker cannot be driven by WebDriver, so this checks the entry point and the
  // initial state; draco-app/tests/live_postgres.rs covers file → COPY → rollback.
  await t.test('offers CSV/JSON import on the table detail', async () => {
    await session.waitFor('the import button', `
      const button = [...document.querySelectorAll('.table-data-controls button')].find((item) => !item.hidden && item.textContent.endsWith('…'));
      if (!button) return false;
      button.click();
      return true;
    `);
    const state = await session.waitFor('the import dialog', `
      const dialog = document.getElementById('table-import-dialog');
      if (dialog.hidden) return null;
      return {
        target: document.getElementById('table-import-target').textContent,
        runDisabled: document.getElementById('table-import-run').disabled,
        previewDisabled: document.getElementById('table-import-preview-button').disabled,
      };
    `);
    assert.deepEqual(state, { target: `${FIXTURE_SCHEMA}.${FIXTURE_TABLE}`, runDisabled: true, previewDisabled: true });
    await session.click('#table-import-close');
    assert.equal(await session.run(`return document.getElementById('table-import-dialog').hidden;`), true);
  });

  await t.test('records executed queries in the history', async () => {
    await session.click('[data-view="query"]');
    await session.click('[data-query-workspace="history"]');
    const entries = await session.waitFor('the history list', `
      const items = [...document.querySelectorAll('#history-list .history-item code')].map((code) => code.textContent);
      return items.length ? items : null;
    `);
    assert.ok(entries.some((sql) => sql.includes(`count(*) AS total FROM ${FIXTURE_SCHEMA}`)), 'the fixture count query is missing from history');
    await session.click('[data-query-workspace="editor"]');
  });
});
