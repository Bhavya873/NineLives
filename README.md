# Nine Lives

A browser driving game where an AI fires a car's 10 cold-gas thrusters so it always lands on its wheels.

Work in progress. `SPEC.md` describes what is being built, `docs/PLAN.md` describes the order it is built in, and `docs/TECHNICAL.md` explains how it all works.

## Developer setup

You need Rust (stable), Python 3.11 or 3.12, and Node 22.12 or newer.

```bash
# Rust
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # must match Cargo.lock
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Python (from the repo root)
python3.12 -m venv .venv && source .venv/bin/activate
pip install -r python/requirements-dev.lock
pip install -e python
maturin develop --release -m crates/nl-py/Cargo.toml
ruff check python && pytest python/tests

# Web
cd web && npm install
npm run wasm          # builds crates/nl-wasm into web/src/wasm
npm run dev           # local page at http://localhost:5173
npm run lint && npm run build
npx playwright install chromium && npm run test:e2e
```

If Chromium is already installed somewhere, point Playwright at it with `PLAYWRIGHT_CHROMIUM_PATH=/path/to/chromium npm run test:e2e`.
