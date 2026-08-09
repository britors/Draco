#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bundle_dir="${DRACO_DEB_BUNDLE_DIR:-${root_dir}/target/release/bundle/deb}"
identifier="$(jq -r .identifier "${root_dir}/src-tauri/tauri.conf.json")"
desktop_name="${identifier}.desktop"
desktop_source="${root_dir}/data/${desktop_name}"
work_dir="$(mktemp -d /tmp/draco-deb.XXXXXX)"
trap 'rm -rf "${work_dir}"' EXIT

fail() {
    echo "Falha: $*" >&2
    exit 1
}

for command_name in desktop-file-validate dpkg-deb jq md5sum; do
    command -v "${command_name}" >/dev/null || fail "comando obrigatório não encontrado: ${command_name}"
done
desktop-file-validate "${desktop_source}"

mapfile -d '' debs < <(find "${bundle_dir}" -maxdepth 1 -type f -name '*.deb' -print0)
((${#debs[@]} == 1)) || fail "esperado exatamente um DEB em ${bundle_dir}; encontrados: ${#debs[@]}"

deb="${debs[0]}"
package_root="${work_dir}/root"
repacked_deb="${work_dir}/$(basename "${deb}")"
dpkg-deb --raw-extract "${deb}" "${package_root}"

applications_dir="${package_root}/usr/share/applications"
find "${applications_dir}" -maxdepth 1 -type f -name '*.desktop' -delete
install -Dm0644 "${desktop_source}" "${applications_dir}/${desktop_name}"

icon_count=0
while IFS= read -r -d '' icon; do
    mv "${icon}" "$(dirname "${icon}")/${identifier}.png"
    ((icon_count += 1))
done < <(find "${package_root}/usr/share/icons/hicolor" -type f -path '*/apps/draco.png' -print0)
((icon_count > 0)) || fail "nenhum ícone draco.png encontrado no DEB"

(
    cd "${package_root}"
    find . -path './DEBIAN' -prune -o -type f -printf '%P\0' \
        | LC_ALL=C sort -z \
        | xargs -0 md5sum
) > "${package_root}/DEBIAN/md5sums"

dpkg-deb --build --root-owner-group "${package_root}" "${repacked_deb}" >/dev/null
mv "${repacked_deb}" "${deb}"
echo "DEB normalizado com a identidade ${identifier}: ${deb}"
