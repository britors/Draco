import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import QRCode from 'qrcode';

const app = await readFile(new URL('../dist/app.js', import.meta.url), 'utf8');
const svg = await readFile(new URL('../dist/pix-donation.svg', import.meta.url), 'utf8');
const payload = app.match(/const PIX_COPY_AND_PASTE = '([^']+)'/)?.[1];

function crc16(value) {
  let crc = 0xffff;
  for (const byte of Buffer.from(value)) {
    crc ^= byte << 8;
    for (let index = 0; index < 8; index += 1) crc = crc & 0x8000 ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
  }
  return crc.toString(16).toUpperCase().padStart(4, '0');
}

test('Pix donation payload and QR asset stay valid and synchronized', async () => {
  assert.ok(payload);
  assert.ok(payload.includes('0014BR.GOV.BCB.PIX0116britors@live.com'));
  assert.equal(payload.slice(-4), crc16(payload.slice(0, -4)));

  const generated = await QRCode.toString(payload, { type: 'svg', errorCorrectionLevel: 'M', margin: 2, width: 280, color: { dark: '#11131B', light: '#FFFFFFFF' } });
  const modules = generated.match(/<path stroke="#11131B" d="([^"]+)"/)?.[1];
  const assetModules = svg.match(/<path stroke="#11131B" d="([^"]+)"/)?.[1];
  assert.ok(modules);
  assert.equal(assetModules, modules);
});
