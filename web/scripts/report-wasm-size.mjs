// Reports the gzipped size of the built wasm against the SPEC 11.7 budget (2.5 MB gzipped).
// Run after `npm run wasm:release && npm run build`. Exits 1 when over the budget.
//
// Usage: node scripts/report-wasm-size.mjs [--max-bytes N]
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const BUDGET_BYTES = 2.5 * 1024 * 1024;
const flag = process.argv.indexOf('--max-bytes');
const maxBytes = flag >= 0 ? Number(process.argv[flag + 1]) : BUDGET_BYTES;
const assetsDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'dist', 'assets');

const wasmFiles = readdirSync(assetsDir).filter((name) => name.endsWith('.wasm'));
if (wasmFiles.length === 0) {
  console.error(`no .wasm in ${assetsDir}; run npm run wasm:release && npm run build first`);
  process.exit(1);
}
let failed = false;
for (const name of wasmFiles) {
  const bytes = readFileSync(join(assetsDir, name));
  const gzipped = gzipSync(bytes, { level: 9 }).length;
  const mb = (n) => (n / (1024 * 1024)).toFixed(2);
  const verdict = gzipped <= maxBytes ? 'ok' : 'OVER BUDGET';
  console.log(
    `${name}: ${mb(bytes.length)} MB raw, ${mb(gzipped)} MB gzipped (budget ${mb(maxBytes)} MB) ${verdict}`,
  );
  failed ||= gzipped > maxBytes;
}
process.exit(failed ? 1 : 0);
