// Interface translation for the local frontend. Catalogs are ES modules bundled with the app, so
// translation works offline and needs no fetch. Only Draco's own interface text goes through
// here: SQL, identifiers, query results and messages coming from PostgreSQL are never translated.
import en from './locales/en.js';
import ptBR from './locales/pt-BR.js';

export const DEFAULT_LOCALE = 'en';
export const CATALOGS = Object.freeze({ en, 'pt-BR': ptBR });

// Picks the first supported locale from the system preference list (navigator.languages), so
// `pt`, `pt-PT` and `pt_BR.UTF-8` all resolve to the pt-BR catalog. Falls back to English.
export function resolveLocale(candidates = [], catalogs = CATALOGS) {
  const supported = Object.keys(catalogs);
  for (const candidate of candidates) {
    if (typeof candidate !== 'string' || !candidate) continue;
    const normalized = candidate.split('.')[0].replace('_', '-').toLowerCase();
    const exact = supported.find((locale) => locale.toLowerCase() === normalized);
    if (exact) return exact;
    const language = normalized.split('-')[0];
    const sameLanguage = supported.find((locale) => locale.toLowerCase().split('-')[0] === language);
    if (sameLanguage) return sameLanguage;
  }
  return DEFAULT_LOCALE;
}

function interpolate(template, params) {
  return template.replace(/\{(\w+)\}/g, (match, name) => (Object.hasOwn(params, name) ? String(params[name]) : match));
}

// Returns `t(key, params)`. A message is either a string with `{placeholders}` or a plural object
// selected by `params.count`: an exact form such as `=0` wins, then the Intl.PluralRules category
// (`one`, `other`, …). Portuguese needs `=0` because CLDR treats 0 as singular there.
// Missing keys fall back to English and then to the key itself, so a gap never blanks the UI.
export function createTranslator(locale, catalogs = CATALOGS) {
  const catalog = catalogs[locale] || catalogs[DEFAULT_LOCALE] || {};
  const fallback = catalogs[DEFAULT_LOCALE] || {};
  const pluralRules = new Intl.PluralRules(locale);
  const numberFormat = new Intl.NumberFormat(locale);
  const t = (key, params = {}) => {
    const message = Object.hasOwn(catalog, key) ? catalog[key] : fallback[key];
    if (message === undefined) return key;
    if (typeof message === 'string') return interpolate(message, params);
    const count = Number(params.count ?? 0);
    const form = message[`=${count}`] ?? message[pluralRules.select(count)] ?? message.other ?? '';
    return interpolate(form, { ...params, count: numberFormat.format(count) });
  };
  t.locale = Object.hasOwn(catalogs, locale) ? locale : DEFAULT_LOCALE;
  return t;
}

const ATTRIBUTE_BINDINGS = [
  ['i18nTitle', 'title'],
  ['i18nAriaLabel', 'aria-label'],
  ['i18nPlaceholder', 'placeholder'],
];

// Applies `data-i18n` (text content) and `data-i18n-title` / `-aria-label` / `-placeholder`
// attributes under `root`. Text is always set with textContent, never as HTML.
export function applyTranslations(root, t) {
  for (const element of root.querySelectorAll('[data-i18n]')) element.textContent = t(element.dataset.i18n);
  for (const [datasetKey, attribute] of ATTRIBUTE_BINDINGS) {
    const selector = `[data-${datasetKey.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`)}]`;
    for (const element of root.querySelectorAll(selector)) element.setAttribute(attribute, t(element.dataset[datasetKey]));
  }
}

// Validation parameters whose value is a `label.*` catalog key (e.g. which object a name belongs
// to) are translated too; every other value, such as identifiers, is inserted as is.
function translateParams(params, t) {
  const translated = {};
  for (const [name, value] of Object.entries(params || {})) {
    translated[name] = typeof value === 'string' && value.startsWith('label.') && t(value) !== value ? t(value) : value;
  }
  return translated;
}

// Builds the text shown for an IPC failure. When the bridge sends a stable message `key`, the
// interface text is translated from it; otherwise the backend message is shown as is, because
// PostgreSQL diagnostics and validation details are never rewritten.
export function errorMessage(error, t, fallbackKey = 'error.operation_error') {
  const key = typeof error?.key === 'string' ? error.key : null;
  if (key && t(key) !== key) return t(key, translateParams(error.params, t));
  if (typeof error?.message === 'string' && error.message.trim()) return error.message;
  return t(fallbackKey);
}
