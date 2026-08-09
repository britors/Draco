#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "${root_dir}/Cargo.toml" | head -n 1)"
bundle_dir="${DRACO_RPM_BUNDLE_DIR:-${root_dir}/target/release/bundle/rpm}"
binary="${DRACO_BINARY:-${root_dir}/target/release/draco}"
rpm_top="$(mktemp -d /tmp/draco-rpm.XXXXXX)"
trap 'rm -rf "${rpm_top}"' EXIT

mkdir -p "${rpm_top}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS,tmp} "${bundle_dir}"
test -x "${binary}"
cp "${root_dir}/packaging/draco-release.spec" "${rpm_top}/SPECS/draco-release.spec"

rpmbuild -bb "${rpm_top}/SPECS/draco-release.spec" \
    --define "_topdir ${rpm_top}" \
    --define "_draco_root ${root_dir}" \
    --define "_draco_binary ${binary}" \
    --define "version ${version}" \
    --define "_tmppath ${rpm_top}/tmp" \
    --define "_rpmdir ${bundle_dir}"

rpm_file="$(find "${bundle_dir}" -mindepth 2 -maxdepth 2 -type f -name "draco-${version}-*.rpm" -print -quit)"
test -n "${rpm_file}"
cp "${rpm_file}" "${bundle_dir}/"
find "${bundle_dir}" -maxdepth 1 -type f -name '*.rpm' -print
