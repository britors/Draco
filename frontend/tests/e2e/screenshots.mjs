// Captures the AppStream/README screenshots from the running application.
//
// Run through `scripts/capture-screenshots.sh`, which prepares a temporary XDG config (light theme,
// one connection) and starts `tauri-driver`. The data shown is a fictitious `store` schema created
// here and dropped at the end, so no real data or credential reaches the images.
import { mkdir, writeFile } from 'node:fs/promises';
import { Session } from './webdriver.mjs';

const CONNECTION_LABEL = process.env.DRACO_E2E_CONNECTION_LABEL || 'Store (staging)';
const OUTPUT_DIR = process.env.DRACO_SCREENSHOT_DIR || 'screenshots';
const WIDTH = 1600;
const HEIGHT = 900;
const SCHEMA = 'store';

const FIXTURE = `
DROP SCHEMA IF EXISTS ${SCHEMA} CASCADE;
CREATE SCHEMA ${SCHEMA};
CREATE TABLE ${SCHEMA}.categories (
  id integer PRIMARY KEY,
  name text NOT NULL UNIQUE
);
CREATE TABLE ${SCHEMA}.products (
  id integer PRIMARY KEY,
  category_id integer NOT NULL REFERENCES ${SCHEMA}.categories(id),
  sku text NOT NULL UNIQUE,
  name text NOT NULL,
  price numeric(10,2) NOT NULL CHECK (price >= 0),
  active boolean NOT NULL DEFAULT true
);
CREATE TABLE ${SCHEMA}.customers (
  id integer PRIMARY KEY,
  name text NOT NULL,
  email text NOT NULL UNIQUE,
  country char(2) NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE ${SCHEMA}.orders (
  id integer PRIMARY KEY,
  customer_id integer NOT NULL REFERENCES ${SCHEMA}.customers(id),
  status text NOT NULL CHECK (status IN ('pending', 'paid', 'shipped', 'cancelled')),
  ordered_at timestamptz NOT NULL,
  total numeric(12,2) NOT NULL DEFAULT 0
);
CREATE INDEX orders_customer_idx ON ${SCHEMA}.orders (customer_id);
CREATE INDEX orders_ordered_at_idx ON ${SCHEMA}.orders (ordered_at DESC);
CREATE TABLE ${SCHEMA}.order_items (
  order_id integer NOT NULL REFERENCES ${SCHEMA}.orders(id) ON DELETE CASCADE,
  product_id integer NOT NULL REFERENCES ${SCHEMA}.products(id),
  quantity integer NOT NULL CHECK (quantity > 0),
  unit_price numeric(10,2) NOT NULL,
  PRIMARY KEY (order_id, product_id)
);
CREATE TABLE ${SCHEMA}.payments (
  id integer PRIMARY KEY,
  order_id integer NOT NULL REFERENCES ${SCHEMA}.orders(id),
  method text NOT NULL,
  amount numeric(12,2) NOT NULL,
  paid_at timestamptz NOT NULL
);
INSERT INTO ${SCHEMA}.categories VALUES
  (1, 'Books'), (2, 'Coffee'), (3, 'Electronics'), (4, 'Garden'), (5, 'Kitchen'), (6, 'Outdoor');
INSERT INTO ${SCHEMA}.products
SELECT g, 1 + g % 6, 'SKU-' || lpad(g::text, 5, '0'),
       (ARRAY['Classic', 'Compact', 'Deluxe', 'Eco', 'Pro', 'Travel'])[1 + g % 6] || ' ' ||
       (ARRAY['Notebook', 'Grinder', 'Lamp', 'Planter', 'Kettle', 'Backpack', 'Speaker', 'Mug'])[1 + g % 8],
       round((5 + (g * 37 % 240))::numeric + 0.99, 2), g % 11 <> 0
FROM generate_series(1, 120) AS g;
INSERT INTO ${SCHEMA}.customers
SELECT g,
       (ARRAY['Ana', 'Bruno', 'Carla', 'Diego', 'Elena', 'Felipe', 'Grace', 'Hugo', 'Iris', 'João'])[1 + g % 10] || ' ' ||
       (ARRAY['Almeida', 'Becker', 'Costa', 'Duarte', 'Evans', 'Ferreira', 'Garcia', 'Hansen'])[1 + g % 8],
       'customer' || g || '@example.com',
       (ARRAY['BR', 'PT', 'US', 'DE', 'AR', 'MX'])[1 + g % 6],
       timestamptz '2026-01-01' + g * interval '7 hours'
FROM generate_series(1, 2000) AS g;
SELECT setseed(0.42);
INSERT INTO ${SCHEMA}.orders
SELECT g, 1 + floor(random() * 2000)::int,
       (ARRAY['paid', 'paid', 'paid', 'shipped', 'shipped', 'pending', 'cancelled'])[1 + floor(random() * 7)::int],
       timestamptz '2026-03-01' + g * interval '23 minutes'
FROM generate_series(1, 12000) AS g;
INSERT INTO ${SCHEMA}.order_items
SELECT o, 1 + floor(random() * 120)::int, 1 + floor(random() * 4)::int, 0
FROM generate_series(1, 12000) AS o, generate_series(1, 1 + floor(random() * 3)::int) AS k
ON CONFLICT DO NOTHING;
UPDATE ${SCHEMA}.order_items i SET unit_price = p.price FROM ${SCHEMA}.products p WHERE p.id = i.product_id;
UPDATE ${SCHEMA}.orders o SET total = s.total
FROM (SELECT order_id, sum(quantity * unit_price) AS total FROM ${SCHEMA}.order_items GROUP BY order_id) s
WHERE s.order_id = o.id;
INSERT INTO ${SCHEMA}.payments
SELECT id, id, (ARRAY['card', 'pix', 'transfer'])[1 + id % 3], total, ordered_at + interval '5 minutes'
FROM ${SCHEMA}.orders WHERE status IN ('paid', 'shipped');
CREATE VIEW ${SCHEMA}.daily_revenue AS
SELECT date_trunc('day', ordered_at)::date AS day, count(*) AS orders, sum(total) AS revenue
FROM ${SCHEMA}.orders WHERE status IN ('paid', 'shipped')
GROUP BY 1;
CREATE FUNCTION ${SCHEMA}.customer_lifetime_value(p_customer integer)
RETURNS numeric
LANGUAGE plpgsql STABLE AS $$
DECLARE
  v_total numeric;
BEGIN
  SELECT coalesce(sum(o.total), 0)
    INTO v_total
    FROM ${SCHEMA}.orders o
   WHERE o.customer_id = p_customer
     AND o.status IN ('paid', 'shipped');
  RETURN round(v_total, 2);
END;
$$;
ANALYZE ${SCHEMA}.categories, ${SCHEMA}.products, ${SCHEMA}.customers, ${SCHEMA}.orders, ${SCHEMA}.order_items, ${SCHEMA}.payments;
`;

const REPORT_QUERY = `SELECT c.name AS category,
       count(DISTINCT o.id) AS orders,
       sum(i.quantity) AS units,
       sum(i.quantity * i.unit_price) AS revenue,
       round(avg(o.total), 2) AS avg_ticket
FROM store.orders o
JOIN store.order_items i ON i.order_id = o.id
JOIN store.products p ON p.id = i.product_id
JOIN store.categories c ON c.id = p.category_id
WHERE o.status IN ('paid', 'shipped')
GROUP BY c.name
ORDER BY revenue DESC;`;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function capture(session, name) {
  // Lets transitions, fonts and virtualized rows settle before the frame is taken.
  await sleep(800);
  await writeFile(`${OUTPUT_DIR}/${name}.png`, await session.screenshot());
  console.log(`captured ${name}.png`);
}

// Scrolls the workspace so `css` sits `offset` pixels below its top edge (0 resets the scroll).
async function scrollTo(session, css, offset = 24) {
  await session.run(`
    const [selector, offset] = arguments;
    const container = document.querySelector('.workspace-content');
    const target = selector ? document.querySelector(selector) : null;
    container.scrollTop = target
      ? target.getBoundingClientRect().top - container.getBoundingClientRect().top + container.scrollTop - offset
      : 0;
  `, css, offset);
}

async function runSql(session, button, sql) {
  await session.click('[data-view="query"]');
  await session.click('[data-query-workspace="editor"]');
  await session.type('#sql-editor', sql);
  await session.run(`
    document.getElementById('result-summary').removeAttribute('data-rows');
    document.getElementById('result-error').textContent = '';
  `);
  await session.click(`#${button}`);
  const outcome = await session.waitFor(`the outcome of ${button}`, `
    if (document.getElementById('run-query').disabled) return null;
    const error = document.getElementById('result-error').textContent.trim();
    if (error) return { error };
    const rows = document.getElementById('result-summary').dataset.rows;
    return rows === undefined ? null : { rows: Number(rows) };
  `);
  if (outcome.error) throw new Error(`SQL failed: ${outcome.error}`);
  return outcome;
}

async function selectConnection(session, selectId) {
  await session.waitFor(`the connection in #${selectId}`, `
    const [id, label] = arguments;
    const select = document.getElementById(id);
    const option = [...select.options].find((item) => item.textContent.includes(label));
    if (!option) return null;
    select.value = option.value;
    select.dispatchEvent(new Event('change', { bubbles: true }));
    return option.value;
  `, selectId, CONNECTION_LABEL);
}

async function openExplorerSchema(session) {
  await session.click('[data-view="explorer"]');
  await session.waitFor('the Explorer connection', `
    const [label] = arguments;
    const item = [...document.querySelectorAll('#explorer-connections button')]
      .find((button) => button.textContent.includes(label));
    if (!item) return false;
    item.click();
    return true;
  `, CONNECTION_LABEL);
  await session.waitFor('the store schema', `
    const button = document.querySelector('.tree-group[data-schema="' + arguments[0] + '"] > .tree-item');
    if (!button) return false;
    button.click();
    return true;
  `, SCHEMA);
  await session.waitFor('the store tables', `
    const group = document.querySelector('.tree-group[data-schema="' + arguments[0] + '"]');
    return group?.dataset.loadState === 'loaded';
  `, SCHEMA);
}

const session = await Session.start();
let fixtureCreated = false;
try {
  await mkdir(OUTPUT_DIR, { recursive: true });
  await session.command('POST', '/window/rect', { width: WIDTH, height: HEIGHT }).catch((error) => {
    console.warn(`could not resize the window: ${error.message}`);
  });

  await session.waitFor('the connection card', `
    const [label] = arguments;
    const card = [...document.querySelectorAll('#connection-list .connection-card')]
      .find((item) => item.querySelector('strong')?.textContent === label);
    if (!card) return false;
    card.querySelector('.connect-action').click();
    return true;
  `, CONNECTION_LABEL);
  await session.waitFor('the connected state', `
    const card = [...document.querySelectorAll('#connection-list .connection-card')]
      .find((item) => item.querySelector('strong')?.textContent === arguments[0]);
    return card?.dataset.state === 'connected';
  `, CONNECTION_LABEL);

  await selectConnection(session, 'query-connection');
  fixtureCreated = true;
  await runSql(session, 'run-script', FIXTURE);

  // 1. SQL editor with a highlighted query and its result grid.
  await runSql(session, 'run-query', REPORT_QUERY);
  await session.run(`document.getElementById('sql-editor').blur();`);
  await scrollTo(session, '#query-connection', 64);
  await capture(session, '01-sql-editor');
  await scrollTo(session, null);

  // 2. Table detail with columns, constraints, indexes and foreign keys.
  await openExplorerSchema(session);
  await session.waitFor('the orders table', `
    const [schema, table] = arguments;
    const group = document.querySelector('.tree-group[data-schema="' + schema + '"]');
    const item = [...(group?.querySelectorAll('.tree-children .tree-item') || [])]
      .find((button) => button.querySelector('.tree-item-label')?.textContent.endsWith(' ' + table));
    if (!item) return false;
    item.click();
    return true;
  `, SCHEMA, 'orders');
  await session.waitFor('the table detail', `
    if (document.getElementById('view-table-detail').hidden) return false;
    const content = document.getElementById('detail-content');
    return content.textContent.includes('CREATE TABLE') && content.querySelectorAll('.table-data-grid tbody tr').length > 0;
  `);
  await session.waitFor('the rows to finish loading', `return !document.getElementById('detail-content').textContent.includes('Loading rows');`);
  await capture(session, '02-table-detail');

  // 3. Entity-relationship diagram of the schema.
  await openExplorerSchema(session);
  await session.click('#open-erd');
  await session.waitFor('the ERD', `return document.querySelector('#erd-content svg, #erd-content canvas, #erd-content .erd-canvas') !== null;`);
  await capture(session, '03-erd');

  // 4. Dashboard with database KPIs.
  await session.click('[data-view="dashboard"]');
  await selectConnection(session, 'dashboard-connection');
  await session.waitFor('the dashboard', `return !document.querySelector('#dashboard-content .empty-state') && document.getElementById('dashboard-content').children.length > 0;`);
  await capture(session, '04-dashboard');

  // 5. Programming workspace editing a PL/pgSQL function.
  await session.click('[data-view="programming"]');
  await selectConnection(session, 'programming-connection');
  await session.waitFor('the store schema option', `
    const select = document.getElementById('programming-schema');
    if (select.disabled || ![...select.options].some((option) => option.value === arguments[0])) return false;
    select.value = arguments[0];
    select.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  `, SCHEMA);
  await session.waitFor('the function editor', `
    const row = [...document.querySelectorAll('.programming-object')]
      .find((item) => item.querySelector('strong')?.textContent === arguments[0]);
    if (!row) return false;
    row.querySelectorAll('.programming-object-actions button')[1].click();
    return true;
  `, 'customer_lifetime_value');
  await session.waitFor('the function source', `return document.getElementById('programming-editor').value.includes('v_total');`);
  await session.run(`document.getElementById('programming-editor').blur();`);
  await scrollTo(session, '.programming-editor-panel');
  await capture(session, '05-programming');
} finally {
  if (fixtureCreated) await runSql(session, 'run-script', `DROP SCHEMA IF EXISTS ${SCHEMA} CASCADE`).catch(() => {});
  await session.quit();
}
