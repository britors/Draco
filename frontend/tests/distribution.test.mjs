import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8');
const [workspace, frontend, tauri, desktop, metainfo, aur, spec, releasePending] = await Promise.all([
  read('../../Cargo.toml'),
  read('../package.json'),
  read('../../src-tauri/tauri.conf.json'),
  read('../../data/org.lyraos.Draco.desktop'),
  read('../../data/org.lyraos.Draco.metainfo.xml'),
  read('../../aur/PKGBUILD'),
  read('../../packaging/obs/postgres-draco.spec'),
  read('../../packaging/RELEASE_PENDING.md'),
]);

const workspaceVersion = workspace.match(/\[workspace\.package\][\s\S]*?version = "([^"]+)"/)?.[1];
const frontendManifest = JSON.parse(frontend);
const tauriConfig = JSON.parse(tauri);
const aurVersion = aur.match(/^pkgver=(.+)$/m)?.[1];
const rpmVersion = spec.match(/^Version:\s*(\S+)/m)?.[1];
const appstreamVersion = metainfo.match(/<release version="([^"]+)"/)?.[1];

test('development manifests stay on one version', () => {
  assert.ok(workspaceVersion);
  assert.equal(frontendManifest.version, workspaceVersion);
  assert.equal(tauriConfig.version, workspaceVersion);
});

test('published Linux package metadata describes one immutable release', () => {
  assert.ok(aurVersion);
  assert.equal(rpmVersion, aurVersion);
  assert.equal(appstreamVersion, aurVersion);
  assert.match(aur, /sha256sums=\('[a-f0-9]{64}'\)/);
  assert.doesNotMatch(aur, /sha256sums=\('SKIP'\)/);
  if (aurVersion !== workspaceVersion) {
    assert.match(releasePending, new RegExp(`desenvolvimento está em \`${workspaceVersion}\``));
    assert.match(releasePending, new RegExp(`ainda apontam para\\s+\`${aurVersion}\``));
  }
});

test('installed identity is consistent across Tauri, desktop and AppStream', () => {
  const appId = tauriConfig.identifier;
  assert.equal(appId, 'org.lyraos.Draco');
  assert.match(desktop, /^Exec=draco$/m);
  assert.match(desktop, new RegExp(`^Icon=${appId}$`, 'm'));
  assert.match(desktop, /^Terminal=false$/m);
  assert.match(metainfo, new RegExp(`<id>${appId}</id>`));
  assert.match(metainfo, new RegExp(`<launchable type="desktop-id">${appId}\\.desktop</launchable>`));
  assert.match(metainfo, /<binary>draco<\/binary>/);
});

test('official packages contain Tauri runtime dependencies without GTK fallback dependencies', () => {
  for (const dependency of ['webkit2gtk-4.1', 'gtk3', 'openssl', 'librsvg', 'xdg-desktop-portal']) {
    assert.match(aur, new RegExp(`'${dependency}'`));
  }
  assert.match(spec, /^BuildRequires:\s+pkgconfig\(webkit2gtk-4\.1\)$/m);
  assert.match(spec, /^Requires:\s+xdg-desktop-portal$/m);
  for (const fallbackDependency of ['gtk4', 'libadwaita', 'gtksourceview-5']) {
    assert.doesNotMatch(aur, new RegExp(`'${fallbackDependency}'`));
    assert.doesNotMatch(spec, new RegExp(`pkgconfig\\(${fallbackDependency}`));
  }
});
