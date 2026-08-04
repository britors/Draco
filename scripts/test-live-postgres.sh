#!/usr/bin/env bash
set -euo pipefail

# This harness supplies only connection metadata. The password is resolved by the Rust tests
# from Secret Service under DRACO_TEST_CONN_ID and is never placed in the environment here.
: "${DRACO_TEST_CONN_ID:=torven-local}"
: "${DRACO_TEST_HOST:=localhost}"
: "${DRACO_TEST_DB:=torven}"
: "${DRACO_TEST_USER:=torven}"
export DRACO_TEST_CONN_ID DRACO_TEST_HOST DRACO_TEST_DB DRACO_TEST_USER

pg_isready -h "$DRACO_TEST_HOST" -p 5432
cargo test -p draco-core --test live_postgres -- --ignored --nocapture
cargo test -p draco-app --test live_postgres -- --ignored --nocapture
