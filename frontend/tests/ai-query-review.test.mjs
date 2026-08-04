import assert from 'node:assert/strict';
import { test } from 'node:test';

import { AI_QUERY_REVIEW_FOCUSES, buildAiQueryReviewMessage } from '../dist/ai-query-review.js';

test('AI query review exposes the same four focuses as the GTK editor', () => {
  assert.deepEqual(AI_QUERY_REVIEW_FOCUSES, ['general', 'performance', 'security', 'readability']);
});

test('general review asks for security, performance and readability without executing writes', () => {
  const message = buildAiQueryReviewMessage('general', ' SELECT * FROM users; ', 'Check tenant isolation.');
  for (const dimension of ['segurança', 'performance', 'legibilidade']) assert.match(message, new RegExp(dimension));
  assert.match(message, /<sql_nao_confiavel>\nSELECT \* FROM users;\n<\/sql_nao_confiavel>/);
  assert.match(message, /Não execute DDL ou DML/);
  assert.match(message, /Contexto adicional do usuário:\nCheck tenant isolation\./);
});

test('review focus falls back to general and rejects an empty editor', () => {
  assert.match(buildAiQueryReviewMessage('unknown', 'SELECT 1'), /revisão completa/);
  assert.throws(() => buildAiQueryReviewMessage('security', '   '), /SQL is required/);
});
