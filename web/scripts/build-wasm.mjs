// Builds crates/nl-wasm into web/src/wasm so Vite can import it.
//
// Usage:
//   node scripts/build-wasm.mjs            debug build (fast to compile)
//   node scripts/build-wasm.mjs --release  small build: wasm-release profile + wasm-opt
//
// Needs `wasm-bindgen` on the PATH, at the same version as the wasm-bindgen
// crate in Cargo.lock (install with `cargo install wasm-bindgen-cli --version <x>`).
import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const isRelease = process.argv.includes('--release');
const webDir = join(dirname(fileURLToPath(import.meta.url)), '..');
const repoDir = join(webDir, '..');
const outDir = join(webDir, 'src', 'wasm');
const profile = isRelease ? 'wasm-release' : 'dev';
const profileDir = isRelease ? 'wasm-release' : 'debug';

function run(command, args, cwd) {
  console.log(`> ${command} ${args.join(' ')}`);
  execFileSync(command, args, { cwd, stdio: 'inherit' });
}

run(
  'cargo',
  ['build', '-p', 'nl-wasm', '--target', 'wasm32-unknown-unknown', '--profile', profile],
  repoDir,
);

const wasmFile = join(repoDir, 'target', 'wasm32-unknown-unknown', profileDir, 'nl_wasm.wasm');
run('wasm-bindgen', [wasmFile, '--target', 'web', '--out-dir', outDir], repoDir);

if (isRelease) {
  // wasm-opt comes from the `binaryen` npm package. SPEC §11.7 asks for -O3.
  const bindgenOutput = join(outDir, 'nl_wasm_bg.wasm');
  run('npx', ['wasm-opt', '-O3', bindgenOutput, '-o', bindgenOutput], webDir);
}
