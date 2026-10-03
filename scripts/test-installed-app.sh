#!/usr/bin/env bash
set -euo pipefail

# Drives the installed Draco package through WebDriver against a real PostgreSQL server.
#
# Like test-live-postgres.sh, this harness supplies only connection metadata. The app resolves the
# password from the credential store (service `draco`, entry `password:$DRACO_TEST_CONN_ID`), so the
# password never appears in the environment, the temporary config or the logs.
#
# Requirements: the package under test, `tauri-driver` (`cargo install tauri-driver --locked`),
# `WebKitWebDriver` (Ubuntu: webkit2gtk-driver) and Node.js 22+. On a headless host, run the script
# under `xvfb-run` or `dbus-run-session` with a display available.
: "${DRACO_TEST_CONN_ID:=torven-local}"
: "${DRACO_TEST_HOST:=localhost}"
: "${DRACO_TEST_PORT:=5432}"
: "${DRACO_TEST_DB:=torven}"
: "${DRACO_TEST_USER:=torven}"
: "${DRACO_E2E_APP:=/usr/bin/draco}"
: "${DRACO_E2E_DRIVER_PORT:=4444}"
export DRACO_E2E_APP
export DRACO_E2E_DRIVER_URL="http://127.0.0.1:${DRACO_E2E_DRIVER_PORT}"
export DRACO_E2E_CONNECTION_LABEL="Draco E2E"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

for tool in tauri-driver WebKitWebDriver node pg_isready; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 2; }
done
[[ -x "$DRACO_E2E_APP" ]] || { echo "application not found: $DRACO_E2E_APP" >&2; exit 2; }
for value in "$DRACO_TEST_CONN_ID" "$DRACO_TEST_HOST" "$DRACO_TEST_DB" "$DRACO_TEST_USER"; do
  [[ "$value" =~ ^[A-Za-z0-9._@-]+$ ]] || { echo "connection metadata contains unsupported characters" >&2; exit 2; }
done
[[ "$DRACO_TEST_PORT" =~ ^[0-9]+$ ]] || { echo "DRACO_TEST_PORT must be numeric" >&2; exit 2; }

pg_isready -h "$DRACO_TEST_HOST" -p "$DRACO_TEST_PORT"

# A throwaway XDG config keeps the user's real connections, history and preferences untouched.
# The credential store is not affected by XDG_CONFIG_HOME, so the stored password is reused.
work_dir="$(mktemp -d)"
driver_pid=""
cleanup() {
  [[ -n "$driver_pid" ]] && kill "$driver_pid" 2>/dev/null || true
  rm -rf "$work_dir"
}
trap cleanup EXIT

export XDG_CONFIG_HOME="$work_dir/config"
mkdir -p "$XDG_CONFIG_HOME/draco"
cat > "$XDG_CONFIG_HOME/draco/connections.toml" <<EOF
[[connections]]
id = "$DRACO_TEST_CONN_ID"
label = "$DRACO_E2E_CONNECTION_LABEL"
host = "$DRACO_TEST_HOST"
port = $DRACO_TEST_PORT
database = "$DRACO_TEST_DB"
user = "$DRACO_TEST_USER"
EOF

tauri-driver --port "$DRACO_E2E_DRIVER_PORT" >"$work_dir/tauri-driver.log" 2>&1 &
driver_pid=$!
for _ in $(seq 1 50); do
  curl -fsS "$DRACO_E2E_DRIVER_URL/status" >/dev/null 2>&1 && break
  sleep 0.2
done

if ! node --test "$repo_root/frontend/tests/e2e/installed-app.e2e.mjs"; then
  echo "--- tauri-driver log" >&2
  cat "$work_dir/tauri-driver.log" >&2
  exit 1
fi
