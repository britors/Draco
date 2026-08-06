// Structural helpers for the Programming area's function/procedure editor: split a
// CREATE FUNCTION/PROCEDURE definition (in the canonical shape pg_get_functiondef emits)
// into an editable header (schema, name, parameters, returns, language, extra clauses)
// plus a body, and reassemble the two back into one script for save/validate/GitHub.
//
// Parsing real-world DDL is inherently best-effort (types can be multi-word, parameters
// can omit a name, extra clauses vary). Instead of trying to be a full SQL parser,
// sliceFunctionDdl() verifies its own work: it reassembles what it parsed and only
// returns a result if that reassembly matches the original text once whitespace and
// identifier quoting are normalized away. Anything it can't confidently round-trip
// (e.g. LANGUAGE C, non dollar-quoted bodies, unusual formatting) returns null so the
// caller can fall back to the plain full-DDL editor instead of risking a corrupted save.

function quoteIdent(value) {
  return `"${String(value).replaceAll('"', '""')}"`;
}

function unquoteIdent(token) {
  if (token.startsWith('"') && token.endsWith('"')) return token.slice(1, -1).replaceAll('""', '"');
  return token;
}

const IDENT_PATTERN = '(?:"(?:[^"]|"")+"|[A-Za-z_][A-Za-z0-9_$]*)';

function skipQuoted(text, index, quote) {
  let i = index + 1;
  while (i < text.length) {
    if (text[i] === quote) {
      if (text[i + 1] === quote) { i += 2; continue; }
      return i;
    }
    i += 1;
  }
  return text.length - 1;
}

/** Index of the ')' matching the '(' at openIndex, skipping over quoted strings/identifiers. */
function findMatchingParen(text, openIndex) {
  let depth = 0;
  for (let i = openIndex; i < text.length; i += 1) {
    const c = text[i];
    if (c === "'" || c === '"') { i = skipQuoted(text, i, c); continue; }
    if (c === '(') depth += 1;
    else if (c === ')') { depth -= 1; if (depth === 0) return i; }
  }
  return -1;
}

/** Splits a comma-separated list at top level only (not inside parens/quotes). */
function splitTopLevel(text, separator) {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i += 1) {
    const c = text[i];
    if (c === "'" || c === '"') { i = skipQuoted(text, i, c); continue; }
    if (c === '(') depth += 1;
    else if (c === ')') depth -= 1;
    else if (depth === 0 && c === separator) { parts.push(text.slice(start, i)); start = i + 1; }
  }
  parts.push(text.slice(start));
  return parts;
}

/** Finds the top-level ' DEFAULT ' keyword or '=' sign that separates a param's type from its default. */
function splitParamDefault(text) {
  let depth = 0;
  for (let i = 0; i < text.length; i += 1) {
    const c = text[i];
    if (c === "'" || c === '"') { i = skipQuoted(text, i, c); continue; }
    if (c === '(') { depth += 1; continue; }
    if (c === ')') { depth -= 1; continue; }
    if (depth !== 0) continue;
    if (c === '=') return { rest: text.slice(0, i).trim(), value: text.slice(i + 1).trim() };
    if (text.slice(i, i + 7).toUpperCase() === 'DEFAULT') {
      const before = i === 0 || /\s/.test(text[i - 1]);
      const after = text[i + 7] === undefined || /\s/.test(text[i + 7]);
      if (before && after) return { rest: text.slice(0, i).trim(), value: text.slice(i + 7).trim() };
    }
  }
  return { rest: text.trim(), value: null };
}

const PARAM_MODES = ['IN', 'OUT', 'INOUT', 'VARIADIC'];

/** Parses one raw parameter (already split from the top-level comma list) into {mode, name, type, default}. */
export function parseFunctionParameter(raw) {
  let text = raw.trim();
  let mode = null;
  const modeMatch = text.match(/^(IN|OUT|INOUT|VARIADIC)\s+/i);
  if (modeMatch) {
    mode = modeMatch[1].toUpperCase();
    text = text.slice(modeMatch[0].length);
  }

  const { rest, value } = splitParamDefault(text);
  text = rest.trim();

  const words = text.split(/\s+/).filter(Boolean);
  let name = null;
  let type;
  if (words.length > 1 && new RegExp(`^${IDENT_PATTERN}$`).test(words[0])) {
    name = unquoteIdent(words[0]);
    type = words.slice(1).join(' ');
  } else {
    type = text;
  }

  return { mode, name, type: type.trim(), default: value };
}

/** Splits a raw parameter-list string (the text between the function's outer parens) into parameter objects. */
export function parseFunctionParameters(paramsRaw) {
  const trimmed = paramsRaw.trim();
  if (!trimmed) return [];
  return splitTopLevel(trimmed, ',').map((part) => parseFunctionParameter(part));
}

/** Formats a single parameter object back into PostgreSQL parameter syntax. */
export function formatFunctionParameter(param) {
  const pieces = [];
  if (param.mode) pieces.push(param.mode);
  if (param.name) pieces.push(quoteIdent(param.name));
  pieces.push(param.type || 'text');
  let out = pieces.join(' ');
  if (param.default) out += ` DEFAULT ${param.default}`;
  return out;
}

/** Formats a list of parameter objects into the raw text that goes between the function's parens. */
export function formatFunctionParameters(params) {
  return params.map(formatFunctionParameter).join(', ');
}

function pickDollarTag(body, preferred) {
  if (preferred && !body.includes(preferred)) return preferred;
  const candidates = ['$$', '$function$', '$body$', '$def$'];
  for (const candidate of candidates) {
    if (!body.includes(candidate)) return candidate;
  }
  let n = 1;
  while (body.includes(`$tag${n}$`)) n += 1;
  return `$tag${n}$`;
}

/**
 * Builds a full `CREATE OR REPLACE FUNCTION|PROCEDURE ...` script from a structured header
 * and a body (the text that goes between the AS $tag$ ... $tag$ delimiters).
 */
export function assembleFunctionDdl(header, body) {
  const kind = header.kind === 'procedure' ? 'PROCEDURE' : 'FUNCTION';
  const qualifiedName = header.schema ? `${quoteIdent(header.schema)}.${quoteIdent(header.name)}` : quoteIdent(header.name);
  const paramsRaw = formatFunctionParameters(header.params || []);
  const tag = pickDollarTag(body, header.tag);
  const lines = [`CREATE OR REPLACE ${kind} ${qualifiedName}(${paramsRaw})`];
  if (kind === 'FUNCTION') lines.push(`RETURNS ${(header.returns || 'void').trim()}`);
  lines.push(`LANGUAGE ${(header.language || 'plpgsql').trim()}`);
  if (header.extra && header.extra.trim()) lines.push(header.extra.trim());
  lines.push(`AS ${tag}`);
  lines.push(body);
  lines.push(`${tag};`);
  return lines.join('\n');
}

function normalizeForCompare(text) {
  return text.replaceAll('"', '').replace(/\s+/g, ' ').trim().replace(/;\s*$/, '');
}

/**
 * Attempts to split an existing CREATE FUNCTION/PROCEDURE script into a structured header
 * (schema, name, raw parameter text, returns, language, any other clause verbatim) and a
 * body. Returns null when the DDL can't be reassembled to match the original closely enough
 * to trust — callers should fall back to full-DDL editing in that case.
 */
export function sliceFunctionDdl(ddl, kind) {
  const text = ddl.replace(/\r\n/g, '\n');
  const kindWord = kind === 'procedure' ? 'PROCEDURE' : 'FUNCTION';
  const headRe = new RegExp(`^\\s*CREATE\\s+(?:OR\\s+REPLACE\\s+)?${kindWord}\\s+`, 'i');
  const headMatch = text.match(headRe);
  if (!headMatch) return null;

  const parenIndex = text.indexOf('(', headMatch[0].length);
  if (parenIndex === -1) return null;
  const qualifiedRaw = text.slice(headMatch[0].length, parenIndex).trim();
  const qualifiedMatch = qualifiedRaw.match(new RegExp(`^(${IDENT_PATTERN})(?:\\.(${IDENT_PATTERN}))?$`));
  if (!qualifiedMatch) return null;
  const schema = qualifiedMatch[2] ? unquoteIdent(qualifiedMatch[1]) : null;
  const name = unquoteIdent(qualifiedMatch[2] || qualifiedMatch[1]);
  if (!name) return null;

  const closeParen = findMatchingParen(text, parenIndex);
  if (closeParen === -1) return null;
  const paramsRaw = text.slice(parenIndex + 1, closeParen).trim();

  let rest = text.slice(closeParen + 1);
  let returns = null;
  if (kind !== 'procedure') {
    const returnsMatch = rest.match(/^\s*RETURNS\s+/i);
    if (!returnsMatch) return null;
    const afterReturns = rest.slice(returnsMatch[0].length);
    const langMatch = afterReturns.match(/\bLANGUAGE\b/i);
    if (!langMatch) return null;
    returns = afterReturns.slice(0, langMatch.index).trim();
    rest = afterReturns.slice(langMatch.index);
  } else {
    const langMatch = rest.match(/\bLANGUAGE\b/i);
    if (!langMatch) return null;
    rest = rest.slice(langMatch.index);
  }

  const langValueMatch = rest.match(new RegExp(`^LANGUAGE\\s+(${IDENT_PATTERN})`, 'i'));
  if (!langValueMatch) return null;
  const language = unquoteIdent(langValueMatch[1]);
  rest = rest.slice(langValueMatch[0].length);

  const asMatch = rest.match(/\bAS\b/i);
  if (!asMatch) return null;
  const extra = rest.slice(0, asMatch.index).trim();
  const afterAs = rest.slice(asMatch.index + asMatch[0].length);

  const tagMatch = afterAs.match(/^\s*(\$[A-Za-z0-9_]*\$)/);
  if (!tagMatch) return null;
  const tag = tagMatch[1];
  const bodyStart = tagMatch[0].length;
  const closeIdx = afterAs.indexOf(tag, bodyStart);
  if (closeIdx === -1) return null;
  const body = afterAs.slice(bodyStart, closeIdx);

  let tail = afterAs.slice(closeIdx + tag.length).trim();
  if (tail.endsWith(';')) tail = tail.slice(0, -1).trim();
  if (tail) return null;

  const params = parseFunctionParameters(paramsRaw);
  const header = { kind: kind === 'procedure' ? 'procedure' : 'function', schema, name, paramsRaw, params, returns, language, extra, tag };

  const reassembled = assembleFunctionDdl(header, body);
  if (normalizeForCompare(reassembled) !== normalizeForCompare(text)) return null;

  return { header, body };
}
