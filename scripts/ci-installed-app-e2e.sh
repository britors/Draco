#!/usr/bin/env bash
set -euo pipefail

# CI wrapper around test-installed-app.sh for a disposable Ubuntu runner.
#
# Run it under `dbus-run-session` so the throwaway Secret Service lives only as long as the test.
# It creates the PostgreSQL role with a password generated here, stores that password in a headless
# gnome-keyring under the same attributes `keyring` uses (service `draco`, username
# `password:<id>`), and then lets the app resolve it. The password only travels through stdin and
# a non-exported shell variable: never the environment, the command line or the logs.
: "${DRACO_TEST_CONN_ID:=draco-e2e-ci}"
: "${DRACO_TEST_DB:=draco_e2e}"
: "${DRACO_TEST_USER:=draco_e2e}"
export DRACO_TEST_CONN_ID DRACO_TEST_DB DRACO_TEST_USER

[[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]] || { echo "run this script under dbus-run-session" >&2; exit 2; }
for tool in gnome-keyring-daemon secret-tool xvfb-run openssl psql; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 2; }
done
for value in "$DRACO_TEST_CONN_ID" "$DRACO_TEST_DB" "$DRACO_TEST_USER"; do
  [[ "$value" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "connection metadata contains unsupported characters" >&2; exit 2; }
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# An empty login password creates and unlocks the default collection on a fresh runner.
printf '' | gnome-keyring-daemon --unlock --components=secrets >/dev/null

password="$(openssl rand -hex 24)"
sudo -u postgres psql --quiet -v ON_ERROR_STOP=1 >/dev/null <<SQL
CREATE ROLE "$DRACO_TEST_USER" LOGIN PASSWORD '$password';
CREATE DATABASE "$DRACO_TEST_DB" OWNER "$DRACO_TEST_USER";
SQL
printf '%s' "$password" | secret-tool store --label="Draco E2E" \
  service draco username "password:$DRACO_TEST_CONN_ID"
unset password

xvfb-run --auto-servernum --server-args="-screen 0 1600x1000x24" \
  "$repo_root/scripts/test-installed-app.sh"
