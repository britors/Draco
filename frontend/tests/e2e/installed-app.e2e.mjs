// End-to-end smoke of the installed Draco package against a real PostgreSQL server.
//
// Run through `scripts/test-installed-app.sh`, which starts `tauri-driver`, points the app at a
// temporary XDG config containing one connection, and keeps the password in the credential store.
// This file talks plain W3C WebDriver over HTTP so the frontend keeps zero runtime dependencies.
import test from 'node:test';
import assert from 'node:assert/strict';

const DRIVER = process.env.DRACO_E2E_DRIVER_URL || 'http://127.0.0.1:4444';
const APPLICATION = process.env.DRACO_E2E_APP || '/usr/bin/draco';
const CONNECTION_LABEL = process.env.DRACO_E2E_CONNECTION_LABEL || 'Draco E2E';
const ELEMENT = 'element-6066-11e4-a52e-4f735466cecf';
const TIMEOUT_MS = Number(process.env.DRACO_E2E_TIMEOUT_MS || 30000);

async function webdriver(method, path, body) {
  const response = await fetch(`${DRIVER}${path}`, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    const error = payload?.value?.error || response.status;
    const message = payload?.value?.message || response.statusText;
    throw new Error(`WebDriver ${method} ${path} failed: ${error} ${message}`);
  }
  return payload.value;
}

class Session {
  static async start() {
    const value = await webdriver('POST', '/session', {
      capabilities: { alwaysMatch: { 'tauri:options': { application: APPLICATION } } },
    });
    return new Session(value.sessionId);
  }

  constructor(id) { this.id = id; }

  command(method, path, body) { return webdriver(method, `/session/${this.id}${path}`, body); }

  async quit() { await webdriver('DELETE', `/session/${this.id}`).catch(() => {}); }

  // Runs a synchronous script in the webview and returns its JSON-serializable result.
  run(script, ...args) { return this.command('POST', '/execute/sync', { script, args }); }

  async find(css) {
    const value = await this.command('POST', '/element', { using: 'css selector', value: css });
    return value[ELEMENT];
  }

  async click(css) { await this.command('POST', `/element/${await this.find(css)}/click`, {}); }

  async type(css, text) {
    const element = await this.find(css);
    await this.command('POST', `/element/${element}/clear`, {});
    await this.command('POST', `/element/${element}/value`, { text });
  }

  // Polls `script` until it returns a truthy value; the last value is reported on timeout.
  async waitFor(description, script, ...args) {
    const deadline = Date.now() + TIMEOUT_MS;
    let last;
    while (Date.now() < deadline) {
      last = await this.run(script, ...args).catch((error) => `error: ${error.message}`);
      if (last && !String(last).startsWith('error:')) return last;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`Timed out waiting for ${description} (last: ${JSON.stringify(last)})`);
  }
}

// Returns the card of the E2E connection, located by its label rendered with textContent.
const FIND_CARD = `
  const [label] = arguments;
  return [...document.querySelectorAll('#connection-list .connection-card')]
    .find((card) => card.querySelector('strong')?.textContent === label) || null;
`;

test('installed app connects, browses the schema and runs a query', async (t) => {
  const session = await Session.start();
  t.after(() => session.quit());

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
      const status = card?.querySelector('small')?.textContent;
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
      const summary = document.getElementById('result-summary').textContent;
      return /^1 rows/.test(summary) ? summary : null;
    `);
    assert.match(summary, /^1 rows/);
    const grid = await session.run(`return document.getElementById('result-grid').textContent;`);
    assert.match(grid, /answer/);
    assert.match(grid, /42/);
    assert.match(grid, /draco/);
  });
});
