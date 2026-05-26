import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { parsePrismaSchema } from '../prisma/PrismaParser';

const FILE = '/workspace/schema.prisma';

// ── helpers ───────────────────────────────────────────────────────────────────
function parse(content: string) {
  return parsePrismaSchema(FILE, content);
}

// ── datasource ────────────────────────────────────────────────────────────────
describe('parsePrismaSchema — datasource', () => {
  it('returns null datasource for empty content', () => {
    assert.equal(parse('').datasource, null);
  });

  it('parses postgresql provider as isPostgres=true', () => {
    const schema = `
datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}`;
    const { datasource } = parse(schema);
    assert.ok(datasource);
    assert.equal(datasource.provider, 'postgresql');
    assert.ok(datasource.isPostgres);
  });

  it('parses postgres provider as isPostgres=true', () => {
    const { datasource } = parse(`
datasource db {
  provider = "postgres"
  url      = "postgresql://localhost"
}`);
    assert.ok(datasource?.isPostgres);
  });

  it('marks non-postgres providers as isPostgres=false', () => {
    const { datasource } = parse(`
datasource db {
  provider = "mysql"
  url      = "mysql://localhost"
}`);
    assert.ok(datasource);
    assert.equal(datasource.isPostgres, false);
  });

  it('stores the raw url string', () => {
    const { datasource } = parse(`
datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}`);
    assert.ok(datasource?.url.includes('DATABASE_URL'));
  });
});

// ── models ────────────────────────────────────────────────────────────────────
describe('parsePrismaSchema — models', () => {
  it('returns empty models array for schema without models', () => {
    assert.deepEqual(parse('').models, []);
  });

  it('parses a simple model', () => {
    const { models } = parse(`
model User {
  id   Int    @id
  name String
}`);
    assert.equal(models.length, 1);
    assert.equal(models[0].name, 'User');
    assert.equal(models[0].tableName, 'User');
  });

  it('uses @@map for tableName', () => {
    const { models } = parse(`
model User {
  id Int @id
  @@map("users")
}`);
    assert.equal(models[0].tableName, 'users');
  });

  it('uses model name when @@map is absent', () => {
    const { models } = parse(`
model Post {
  id Int @id
}`);
    assert.equal(models[0].tableName, 'Post');
  });

  it('parses multiple models', () => {
    const { models } = parse(`
model User {
  id Int @id
}
model Post {
  id Int @id
}
model Comment {
  id Int @id
}
`);
    assert.equal(models.length, 3);
    assert.deepEqual(models.map(m => m.name), ['User', 'Post', 'Comment']);
  });

  it('parses @id field attribute', () => {
    const { models } = parse(`
model User {
  id Int @id
}`);
    const id = models[0].fields.find(f => f.name === 'id');
    assert.ok(id?.isId);
  });

  it('parses optional field (? suffix)', () => {
    const { models } = parse(`
model User {
  id  Int     @id
  bio String?
}`);
    const bio = models[0].fields.find(f => f.name === 'bio');
    assert.ok(bio?.isOptional);
  });

  it('parses list field ([] suffix)', () => {
    const { models } = parse(`
model User {
  id   Int      @id
  tags String[]
}`);
    const tags = models[0].fields.find(f => f.name === 'tags');
    assert.ok(tags?.isList);
  });

  it('parses @map column alias', () => {
    const { models } = parse(`
model User {
  id        Int      @id
  createdAt DateTime @map("created_at")
}`);
    const f = models[0].fields.find(f => f.name === 'createdAt');
    assert.equal(f?.columnName, 'created_at');
  });

  it('uses field name as columnName when @map is absent', () => {
    const { models } = parse(`
model User {
  id   Int    @id
  name String
}`);
    const f = models[0].fields.find(f => f.name === 'name');
    assert.equal(f?.columnName, 'name');
  });

  it('strips single-line comments before parsing', () => {
    const schema = `
// This is a comment
model User {
  // another comment
  id Int @id // inline
}`;
    const { models } = parse(schema);
    assert.equal(models.length, 1);
    assert.ok(models[0].fields.some(f => f.name === 'id'));
  });

  it('strips block comments before parsing', () => {
    const schema = `/* block comment */
model Post {
  id Int @id
}`;
    const { models } = parse(schema);
    assert.equal(models.length, 1);
    assert.equal(models[0].name, 'Post');
  });

  it('ignores @@-level attributes as fields', () => {
    const { models } = parse(`
model User {
  id  Int    @id
  @@index([id])
  @@unique([id])
}`);
    const fieldNames = models[0].fields.map(f => f.name);
    assert.ok(!fieldNames.includes('@@index'));
    assert.ok(!fieldNames.includes('@@unique'));
  });

  it('stores the filePath unchanged', () => {
    const result = parse('model Foo { id Int @id }');
    assert.equal(result.filePath, FILE);
  });
});
