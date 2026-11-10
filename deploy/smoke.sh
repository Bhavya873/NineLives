#!/usr/bin/env bash
# Smoke test for a running Nine Lives deploy (the Docker image in CI, or Railway later).
# Usage: deploy/smoke.sh http://localhost:8080
#        deploy/smoke.sh https://<your-app>.up.railway.app
# Checks: /healthz says "ok"; / is HTML titled Nine Lives; /viewer.html is 200; the wasm file
# the game bundle points at is served as application/wasm; /assets/ files are cached for a
# year as immutable while / is not.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <base-url>   e.g. $0 http://localhost:8080" >&2
  exit 2
fi
base="${1%/}"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# Prints the response headers of a GET, lower-cased, with CRs removed.
headers_of() {
  curl -sS -D - -o /dev/null "$base$1" | tr -d '\r' | tr 'A-Z' 'a-z'
}

# Prints the first header line named $2 (for example content-type) of path $1.
header_value() {
  headers_of "$1" | grep -m1 "^$2:" || true
}

status_of() {
  curl -sS -o /dev/null -w '%{http_code}' "$base$1"
}

# 1. Health check, the path Railway polls.
health="$(curl -sS "$base/healthz")"
[ "$health" = "ok" ] || fail "/healthz returned '$health', expected 'ok'"
echo "ok   /healthz returns ok"

# 2. The game page.
index_html="$(curl -sS "$base/")"
case "$index_html" in
  *"<title>Nine Lives</title>"*) ;;
  *) fail "/ did not contain <title>Nine Lives</title>; got: $(printf '%s' "$index_html" | head -c 200)" ;;
esac
echo "ok   / is the game page"

# 3. The training viewer page.
code="$(status_of /viewer.html)"
[ "$code" = "200" ] || fail "/viewer.html returned HTTP $code, expected 200"
echo "ok   /viewer.html returns 200"

# 4. Find the wasm file. The page links its JS bundle and the bundle names the wasm file, so
#    look through the linked scripts and, one level down, the scripts they import.
find_wasm_name() {
  local js_paths js_path body imported name
  js_paths="$(printf '%s' "$index_html" | grep -oE '/assets/[A-Za-z0-9_.-]+\.js' | sort -u || true)"
  for js_path in $js_paths; do
    body="$(curl -sS "$base$js_path")"
    name="$(printf '%s' "$body" | grep -oE '[A-Za-z0-9_.-]+\.wasm' | head -n1 || true)"
    if [ -n "$name" ]; then
      echo "$name"
      return 0
    fi
    for imported in $(printf '%s' "$body" | grep -oE '\./[A-Za-z0-9_.-]+\.js' | sort -u || true); do
      name="$(curl -sS "$base/assets/${imported#./}" | grep -oE '[A-Za-z0-9_.-]+\.wasm' | head -n1 || true)"
      if [ -n "$name" ]; then
        echo "$name"
        return 0
      fi
    done
  done
  return 1
}
wasm_name="$(find_wasm_name)" || fail "no .wasm file name found in the JS bundles linked from /"
wasm_path="/assets/$wasm_name"
code="$(status_of "$wasm_path")"
[ "$code" = "200" ] || fail "$wasm_path returned HTTP $code, expected 200"
wasm_type="$(header_value "$wasm_path" content-type)"
case "$wasm_type" in
  "content-type: application/wasm"*) ;;
  *) fail "$wasm_path content-type was '$wasm_type', expected application/wasm" ;;
esac
echo "ok   $wasm_path is served as application/wasm"

# 5. Cache headers: hashed assets are immutable, the page always revalidates.
asset_cache="$(header_value "$wasm_path" cache-control)"
case "$asset_cache" in
  *immutable*max-age=31536000* | *max-age=31536000*immutable*) ;;
  *) fail "$wasm_path cache-control was '$asset_cache', expected max-age=31536000 and immutable" ;;
esac
page_cache="$(header_value / cache-control)"
case "$page_cache" in
  *immutable*) fail "/ cache-control was '$page_cache', it must not be immutable" ;;
  *no-cache*) ;;
  *) fail "/ cache-control was '$page_cache', expected no-cache" ;;
esac
echo "ok   /assets/ is immutable, / is no-cache"

echo "All smoke checks passed for $base"
