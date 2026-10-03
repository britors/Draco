import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { CATALOGS, DEFAULT_LOCALE, applyTranslations, createTranslator, errorMessage, resolveLocale } from '../dist/i18n.js';

const [index, app] = await Promise.all([
  readFile(new URL('../dist/index.html', import.meta.url), 'utf8'),
  readFile(new URL('../dist/app.js', import.meta.url), 'utf8'),
]);
const english = CATALOGS[DEFAULT_LOCALE];

test('system locale resolves to a supported catalog with English fallback', () => {
  assert.equal(resolveLocale(['pt-BR', 'en-US']), 'pt-BR');
  assert.equal(resolveLocale(['pt']), 'pt-BR');
  assert.equal(resolveLocale(['pt-PT']), 'pt-BR');
  assert.equal(resolveLocale(['pt_BR.UTF-8']), 'pt-BR');
  assert.equal(resolveLocale(['en-GB', 'pt-BR']), 'en');
  assert.equal(resolveLocale(['de-DE', 'pt-BR']), 'pt-BR');
  assert.equal(resolveLocale(['de-DE']), 'en');
  assert.equal(resolveLocale([]), 'en');
  assert.equal(resolveLocale([undefined, '']), 'en');
});

test('placeholders are replaced and unknown ones are left visible', () => {
  const t = createTranslator('en', { en: { greet: 'Hello {name}, {missing}' } });
  assert.equal(t('greet', { name: 'Draco' }), 'Hello Draco, {missing}');
  assert.equal(t('greet', { name: '{missing}' }), 'Hello {missing}, {missing}');
});

test('plurals follow each locale, with exact forms taking precedence', () => {
  const en = createTranslator('en');
  assert.equal(en('connections.count', { count: 0 }), '0 connections');
  assert.equal(en('connections.count', { count: 1 }), '1 connection');
  assert.equal(en('connections.count', { count: 2 }), '2 connections');
  const pt = createTranslator('pt-BR');
  assert.equal(pt('connections.count', { count: 0 }), 'nenhuma conexão');
  assert.equal(pt('connections.count', { count: 1 }), '1 conexão');
  assert.equal(pt('connections.count', { count: 2 }), '2 conexões');
  assert.equal(pt('connections.count', { count: 1500 }), '1.500 conexões');
  assert.equal(pt('results.summary', { count: 0, ms: 3 }), '0 linhas · 3 ms');
  assert.equal(pt('results.summary', { count: 1, ms: 3 }), '1 linha · 3 ms');
  assert.equal(en('results.summary', { count: 1, ms: 3 }), '1 row · 3 ms');
});

test('missing keys fall back to English, then to the key itself', () => {
  const catalogs = { en: { only: 'English only', shared: 'Shared' }, 'pt-BR': { shared: 'Compartilhado' } };
  const t = createTranslator('pt-BR', catalogs);
  assert.equal(t('shared'), 'Compartilhado');
  assert.equal(t('only'), 'English only');
  assert.equal(t('nowhere'), 'nowhere');
  assert.equal(createTranslator('fr', catalogs).locale, 'en');
});

test('catalogs have identical keys and every plural has an other form', () => {
  const englishKeys = Object.keys(english).sort();
  for (const [locale, catalog] of Object.entries(CATALOGS)) {
    assert.deepEqual(Object.keys(catalog).sort(), englishKeys, `${locale} keys differ from en`);
    for (const [key, message] of Object.entries(catalog)) {
      if (typeof message === 'string') {
        assert.ok(message.trim(), `${locale}:${key} is empty`);
        const placeholders = (text) => [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
        if (typeof english[key] === 'string') assert.deepEqual(placeholders(message), placeholders(english[key]), `${locale}:${key} placeholders differ`);
      } else {
        assert.equal(typeof message.other, 'string', `${locale}:${key} needs an other form`);
      }
    }
  }
});

test('every key referenced by the shell and app.js exists in the catalog', () => {
  const htmlKeys = [...index.matchAll(/data-i18n(?:-title|-aria-label|-placeholder)?="([^"]+)"/g)].map((match) => match[1]);
  assert.ok(htmlKeys.length > 40);
  const staticKeys = [...app.matchAll(/\bt\('([^']+)'/g)].map((match) => match[1]);
  // Fallback keys handed to errorMessage(error, t, 'key') are catalog keys too.
  staticKeys.push(...[...app.matchAll(/errorMessage\([^)]*?, t, '([^']+)'\)/g)].map((match) => match[1]));
  assert.ok(staticKeys.length > 20);
  for (const key of [...htmlKeys, ...staticKeys]) assert.ok(Object.hasOwn(english, key), `missing catalog key ${key}`);
  // Keys built at runtime from fixed enumerations.
  for (const view of ['connections', 'explorer', 'programming', 'dashboard', 'admin', 'assistant', 'query', 'preferences', 'table-detail', 'erd']) {
    assert.ok(Object.hasOwn(english, `nav.${view}`), `missing nav.${view}`);
  }
  for (const kind of ['function', 'procedure', 'trigger', 'view', 'sequence', 'index', 'table']) {
    assert.ok(Object.hasOwn(english, `kind.${kind}`), `missing kind.${kind}`);
  }
  for (const kind of ['keyword', 'schema', 'table', 'view', 'function', 'column']) {
    assert.ok(Object.hasOwn(english, `autocomplete.${kind}`), `missing autocomplete.${kind}`);
  }
  for (const kind of ['function', 'procedure', 'trigger', 'sequence', 'view']) {
    assert.ok(Object.hasOwn(english, `explorer.deleteKind.${kind}`), `missing explorer.deleteKind.${kind}`);
  }
  for (const state of ['disconnected', 'connecting', 'connected', 'error']) {
    assert.ok(Object.hasOwn(english, `connections.state.${state}`), `missing connections.state.${state}`);
  }
});

test('applyTranslations sets text and attributes without parsing HTML', () => {
  const element = (dataset) => ({ dataset, textContent: 'old', attributes: {}, setAttribute(name, value) { this.attributes[name] = value; } });
  const text = element({ i18n: 'markup' });
  const titled = element({ i18nTitle: 'markup', i18nAriaLabel: 'plain' });
  const field = element({ i18nPlaceholder: 'plain' });
  const root = {
    querySelectorAll(selector) {
      return { '[data-i18n]': [text], '[data-i18n-title]': [titled], '[data-i18n-aria-label]': [titled], '[data-i18n-placeholder]': [field] }[selector] || [];
    },
  };
  const t = createTranslator('en', { en: { markup: '<b>bold</b>', plain: 'Plain' } });
  applyTranslations(root, t);
  assert.equal(text.textContent, '<b>bold</b>');
  assert.deepEqual(titled.attributes, { title: '<b>bold</b>', 'aria-label': 'Plain' });
  assert.deepEqual(field.attributes, { placeholder: 'Plain' });
});

test('IPC errors are translated by key but PostgreSQL text is never rewritten', () => {
  const t = createTranslator('pt-BR');
  assert.equal(errorMessage({ code: 'connection_not_active', key: 'error.connection_not_active', message: "Connection 'x' is not connected" }, t), 'A conexão não está ativa.');
  assert.equal(errorMessage({ code: 'backend_error', key: 'error.operation_error', message: 'The requested operation could not be completed' }, t), 'Não foi possível concluir a operação.');
  const postgres = 'ERROR: relation "pedidos" does not exist';
  assert.equal(errorMessage({ code: 'backend_error', message: postgres }, t), postgres);
  assert.equal(errorMessage({ code: 'invalid_input', message: 'Only ALTER SEQUENCE definitions are accepted' }, t), 'Only ALTER SEQUENCE definitions are accepted');
  assert.equal(errorMessage({ code: 'operation_error', key: 'error.not_in_catalog', message: 'English fallback' }, t), 'English fallback');
  assert.equal(errorMessage(new Error(''), t), 'Não foi possível concluir a operação.');
  assert.equal(errorMessage(null, t, 'connectionForm.saveFailed'), 'Não foi possível salvar a conexão.');
});
