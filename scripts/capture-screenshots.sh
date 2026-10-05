#!/usr/bin/env bash
set -euo pipefail

# Captures the AppStream, site and README screenshots from a Draco build through WebDriver.
#
# Same requirements and credential rule as test-installed-app.sh: the password is resolved by the app
# from the credential store (service `draco`, entry `password:$DRACO_TEST_CONN_ID`). The connected role
# must be able to create and drop the fictitious `store` schema used in the images. The app runs with a
# temporary XDG config in light theme and English, so the user's real connections never appear.
: "${DRACO_TEST_CONN_ID:=torven-local}"
: "${DRACO_TEST_HOST:=localhost}"
: "${DRACO_TEST_PORT:=5432}"
: "${DRACO_TEST_DB:=torven}"
: "${DRACO_TEST_USER:=torven}"
: "${DRACO_E2E_APP:=/usr/bin/draco}"
: "${DRACO_E2E_DRIVER_PORT:=4444}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
: "${DRACO_SCREENSHOT_DIR:=$repo_root/site/assets/screenshots}"
export DRACO_E2E_APP DRACO_SCREENSHOT_DIR
export DRACO_E2E_DRIVER_URL="http://127.0.0.1:${DRACO_E2E_DRIVER_PORT}"
export DRACO_E2E_CONNECTION_LABEL="Store (staging)"

: "${DRACO_E2E_TAURI_DRIVER:=$(command -v tauri-driver || echo "$HOME/.cargo/bin/tauri-driver")}"
: "${DRACO_E2E_NATIVE_DRIVER:=$(command -v WebKitWebDriver || true)}"
[[ -x "$DRACO_E2E_TAURI_DRIVER" ]] || { echo "missing required tool: tauri-driver" >&2; exit 2; }
[[ -x "$DRACO_E2E_NATIVE_DRIVER" ]] || { echo "missing required tool: WebKitWebDriver" >&2; exit 2; }
for tool in node pg_isready curl; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 2; }
done
[[ -x "$DRACO_E2E_APP" ]] || { echo "application not found: $DRACO_E2E_APP" >&2; exit 2; }
for value in "$DRACO_TEST_CONN_ID" "$DRACO_TEST_HOST" "$DRACO_TEST_DB" "$DRACO_TEST_USER"; do
  [[ "$value" =~ ^[A-Za-z0-9._@-]+$ ]] || { echo "connection metadata contains unsupported characters" >&2; exit 2; }
done
[[ "$DRACO_TEST_PORT" =~ ^[0-9]+$ ]] || { echo "DRACO_TEST_PORT must be numeric" >&2; exit 2; }

pg_isready -h "$DRACO_TEST_HOST" -p "$DRACO_TEST_PORT"

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
favorite = true
EOF
cat > "$XDG_CONFIG_HOME/draco/settings.toml" <<EOF
theme = "light"
accent = "coral"
check_updates_on_startup = false
EOF

# The app inherits the locale of the driver process.
LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 \
  "$DRACO_E2E_TAURI_DRIVER" --port "$DRACO_E2E_DRIVER_PORT" --native-driver "$DRACO_E2E_NATIVE_DRIVER" >"$work_dir/tauri-driver.log" 2>&1 &
driver_pid=$!
for _ in $(seq 1 50); do
  curl -fsS "$DRACO_E2E_DRIVER_URL/status" >/dev/null 2>&1 && break
  sleep 0.2
done

if ! node "$repo_root/frontend/tests/e2e/screenshots.mjs"; then
  echo "--- tauri-driver log" >&2
  cat "$work_dir/tauri-driver.log" >&2
  exit 1
fi
