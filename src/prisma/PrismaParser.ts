export interface PrismaDataSource {
  provider: string;
  url: string;
  isPostgres: boolean;
}

export interface PrismaField {
  name: string;
  prismaType: string;   // e.g. String, Int, Boolean, DateTime
  columnName: string;   // actual DB column name (from @map or field name)
  isOptional: boolean;
  isId: boolean;
  isList: boolean;      // array fields (relations or native arrays)
}

export interface PrismaModel {
  name: string;
  tableName: string;    // from @@map("...") or model name
  fields: PrismaField[];
}

export interface ParsedPrismaSchema {
  filePath: string;
  datasource: PrismaDataSource | null;
  models: PrismaModel[];
}

export function parsePrismaSchema(filePath: string, content: string): ParsedPrismaSchema {
  const clean = stripComments(content);
  return {
    filePath,
    datasource: parseDatasource(clean),
    models: parseModels(clean),
  };
}

function stripComments(src: string): string {
  let out = '';
  let i = 0;
  while (i < src.length) {
    if (src[i] === '"') {
      // consume string literal verbatim
      out += src[i++];
      while (i < src.length && src[i] !== '"') {
        if (src[i] === '\\') out += src[i++]; // skip escape
        out += src[i++];
      }
      if (i < src.length) out += src[i++]; // closing "
    } else if (src[i] === '/' && src[i + 1] === '/') {
      // line comment — skip to end of line
      while (i < src.length && src[i] !== '\n') i++;
    } else if (src[i] === '/' && src[i + 1] === '*') {
      // block comment — skip to */
      i += 2;
      while (i < src.length && !(src[i] === '*' && src[i + 1] === '/')) i++;
      i += 2;
    } else {
      out += src[i++];
    }
  }
  return out;
}

function parseDatasource(content: string): PrismaDataSource | null {
  const match = content.match(/datasource\s+\w+\s*\{([\s\S]*?)\}/);
  if (!match) return null;
  const block = match[1];
  const provider = (block.match(/\bprovider\s*=\s*"([^"]+)"/) || [])[1] ?? '';
  const urlLine  = (block.match(/\burl\s*=\s*(.+)/) || [])[1]?.trim() ?? '';
  return { provider, url: urlLine, isPostgres: /postgresql|postgres/i.test(provider) };
}

function parseModels(content: string): PrismaModel[] {
  const models: PrismaModel[] = [];
  const lines = content.split('\n');
  let i = 0;
  while (i < lines.length) {
    const modelMatch = lines[i].match(/^\s*model\s+(\w+)\s*\{/);
    if (modelMatch) {
      const name = modelMatch[1];
      let depth = 1;
      const bodyLines: string[] = [];
      i++;
      while (i < lines.length && depth > 0) {
        const l = lines[i];
        for (const ch of l) {
          if (ch === '{') depth++;
          else if (ch === '}') depth--;
        }
        if (depth > 0) bodyLines.push(l);
        i++;
      }
      const body = bodyLines.join('\n');
      const mapMatch = body.match(/@@map\s*\(\s*"([^"]+)"\s*\)/);
      models.push({ name, tableName: mapMatch ? mapMatch[1] : name, fields: parseFields(body) });
    } else {
      i++;
    }
  }
  return models;
}

function parseFields(body: string): PrismaField[] {
  const fields: PrismaField[] = [];
  for (const line of body.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('@@') || trimmed.startsWith('//')) continue;
    const m = trimmed.match(/^(\w+)\s+([\w\[\]?]+)\s*(.*)?$/);
    if (!m) continue;
    const fieldName = m[1];
    const typeRaw   = m[2];
    const attrs     = m[3] ?? '';
    const isList    = typeRaw.includes('[');
    const isOptional = typeRaw.endsWith('?');
    const prismaType = typeRaw.replace(/[\[\]?]/g, '');
    const colMapMatch = attrs.match(/@map\s*\(\s*"([^"]+)"\s*\)/);
    fields.push({
      name: fieldName,
      prismaType,
      columnName: colMapMatch ? colMapMatch[1] : fieldName,
      isOptional,
      isId: /@id\b/.test(attrs),
      isList,
    });
  }
  return fields;
}
