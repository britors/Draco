#!/usr/bin/env bash
set -euo pipefail

# Builds a throwaway SSH topology on a disposable Ubuntu runner and runs draco-core's live_ssh test.
#
#   bastion sshd 127.0.0.1:2222  password only, may forward only to the target sshd
#   target  sshd 127.0.0.1:2223  password or passphrase-protected key, may forward only to PostgreSQL
#   decoy   sshd 127.0.0.1:2224  like the target, but known_hosts records a different host key
#
# Run it under `dbus-run-session`, with the runner's PostgreSQL started. Every password is generated
# here and reaches PostgreSQL, chpasswd, ssh-keygen and the Secret Service through stdin or a 0600
# file, never through the environment, the command line or the logs. The test reads them back from
# the Secret Service under the same attributes `keyring` uses (service `draco`, username
# `<kind>:<id>`), exactly like the app.
: "${DRACO_TEST_CONN_ID:=draco-ssh-ci}"
: "${DRACO_TEST_DB:=draco_ssh}"
: "${DRACO_TEST_USER:=draco_ssh}"
bastion_user=draco_bastion
target_user=draco_target
bastion_port=2222
target_port=2223
decoy_port=2224

[[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]] || { echo "run this script under dbus-run-session" >&2; exit 2; }
for tool in gnome-keyring-daemon secret-tool openssl psql ssh-keygen setsid chpasswd /usr/sbin/sshd; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 2; }
done
for value in "$DRACO_TEST_CONN_ID" "$DRACO_TEST_DB" "$DRACO_TEST_USER"; do
  [[ "$value" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "connection metadata contains unsupported characters" >&2; exit 2; }
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
umask 077
work_dir="$(mktemp -d)"
sshd_dir="$(sudo mktemp -d)"
cleanup() {
  for pid_file in "$sshd_dir"/*.pid; do
    [[ -e "$pid_file" ]] && sudo kill "$(sudo cat "$pid_file")" 2>/dev/null || true
  done
  sudo rm -rf "$sshd_dir"
  rm -rf "$work_dir"
}
trap cleanup EXIT

secret() { openssl rand -hex 24 | tr -d '\n'; }
store_secret() { # kind, value on stdin
  secret-tool store --label="Draco SSH CI $1" service draco username "$1:$DRACO_TEST_CONN_ID"
}

# Headless Secret Service; a random password avoids gnome-keyring's display-only prompter.
secret | gnome-keyring-daemon --unlock --components=secrets >/dev/null

# PostgreSQL role reached only through the tunnel.
pg_password="$(secret)"
sudo -u postgres psql --quiet -v ON_ERROR_STOP=1 >/dev/null <<SQL
CREATE ROLE "$DRACO_TEST_USER" LOGIN PASSWORD '$pg_password';
CREATE DATABASE "$DRACO_TEST_DB" OWNER "$DRACO_TEST_USER";
SQL
printf '%s' "$pg_password" | store_secret password
unset pg_password

# SSH accounts. The target password doubles as the key passphrase, matching how the app passes
# one SSH secret either as the password or as the passphrase of the configured key.
ssh_password="$(secret)"
jump_password="$(secret)"
for user in "$bastion_user" "$target_user"; do
  id "$user" >/dev/null 2>&1 || sudo useradd --create-home --shell /bin/bash "$user"
done
printf '%s:%s\n%s:%s\n' "$bastion_user" "$jump_password" "$target_user" "$ssh_password" | sudo chpasswd
printf '%s' "$ssh_password" | store_secret ssh
printf '%s' "$jump_password" | store_secret jump
unset jump_password

# ssh-keygen reads the passphrase only from a terminal or an askpass helper.
printf '%s' "$ssh_password" > "$work_dir/passphrase"
unset ssh_password
printf '#!/bin/sh\ncat "%s/passphrase"\n' "$work_dir" > "$work_dir/askpass"
chmod 700 "$work_dir/askpass"
SSH_ASKPASS="$work_dir/askpass" SSH_ASKPASS_REQUIRE=force DISPLAY=none \
  setsid -w ssh-keygen -q -t ed25519 -C draco-ssh-ci -f "$work_dir/id_ed25519" </dev/null
rm -f "$work_dir/passphrase" "$work_dir/askpass"
target_home="$(getent passwd "$target_user" | cut -d: -f6)"
sudo install -d -m 700 -o "$target_user" -g "$target_user" "$target_home/.ssh"
sudo install -m 600 -o "$target_user" -g "$target_user" "$work_dir/id_ed25519.pub" "$target_home/.ssh/authorized_keys"

start_sshd() { # name, port, extra config lines
  local name="$1" port="$2" extra="$3"
  sudo ssh-keygen -q -t ed25519 -N '' -C "draco-$name" -f "$sshd_dir/host_$name"
  sudo tee "$sshd_dir/$name.conf" >/dev/null <<EOF
ListenAddress 127.0.0.1
Port $port
HostKey $sshd_dir/host_$name
PidFile $sshd_dir/$name.pid
AuthorizedKeysFile .ssh/authorized_keys
KbdInteractiveAuthentication no
PermitRootLogin no
PermitTTY no
X11Forwarding no
AllowAgentForwarding no
AllowTcpForwarding local
LogLevel VERBOSE
$extra
EOF
  sudo /usr/sbin/sshd -t -f "$sshd_dir/$name.conf"
  sudo /usr/sbin/sshd -f "$sshd_dir/$name.conf" -E "$sshd_dir/$name.log"
}
sudo install -d -m 755 /run/sshd
start_sshd bastion "$bastion_port" "AllowUsers $bastion_user
PasswordAuthentication yes
PubkeyAuthentication no
PermitOpen 127.0.0.1:$target_port"
target_rules="AllowUsers $target_user
PasswordAuthentication yes
PubkeyAuthentication yes
PermitOpen 127.0.0.1:5432"
start_sshd target "$target_port" "$target_rules"
start_sshd decoy "$decoy_port" "$target_rules"

# The bastion and target are learned on first use, like the app does. The decoy gets a recorded
# key that differs from the one it presents, which the tunnel must refuse.
install -d -m 700 "$HOME/.ssh"
ssh-keygen -q -t ed25519 -N '' -C draco-recorded -f "$work_dir/recorded_host"
printf '[127.0.0.1]:%s %s\n' "$decoy_port" "$(cut -d' ' -f1,2 "$work_dir/recorded_host.pub")" >> "$HOME/.ssh/known_hosts"

export DRACO_TEST_CONN_ID DRACO_TEST_DB DRACO_TEST_USER
export DRACO_TEST_HOST=127.0.0.1
export DRACO_TEST_SSH_HOST=127.0.0.1
export DRACO_TEST_SSH_PORT="$target_port"
export DRACO_TEST_SSH_USER="$target_user"
export DRACO_TEST_SSH_KEY="$work_dir/id_ed25519"
export DRACO_TEST_SSH_JUMP_PORT="$bastion_port"
export DRACO_TEST_SSH_JUMP_USER="$bastion_user"
export DRACO_TEST_SSH_MISMATCH_PORT="$decoy_port"

status=0
(cd "$repo_root" && cargo test --locked -p draco-core --test live_ssh -- --ignored --test-threads=1) || status=$?

# Each hop must actually have been used: the bastion forwarded to the target, and the target
# forwarded to PostgreSQL for both direct and jump-host tunnels.
for name in bastion target; do
  sudo grep -q "Accepted .* for draco_" "$sshd_dir/$name.log" || {
    echo "the $name sshd never authenticated a Draco test user" >&2
    status=1
  }
done
if sudo grep -q "Accepted .* for draco_" "$sshd_dir/decoy.log"; then
  echo "the decoy sshd authenticated a user despite the changed host key" >&2
  status=1
fi
if [[ "$status" -ne 0 ]]; then
  for name in bastion target decoy; do
    echo "--- sshd $name log" >&2
    sudo cat "$sshd_dir/$name.log" >&2
  done
fi
exit "$status"
