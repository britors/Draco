import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { test } from 'node:test';

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8');
const [workspace, frontend, tauri, tauriMain, desktop, metainfo, spec, releaseWorkflow] = await Promise.all([
  read('../../Cargo.toml'),
  read('../package.json'),
  read('../../src-tauri/tauri.conf.json'),
  read('../../src-tauri/src/main.rs'),
  read('../../data/org.lyraos.Draco.desktop'),
  read('../../data/org.lyraos.Draco.metainfo.xml'),
  read('../../packaging/obs/postgres-draco.spec'),
  read('../../.github/workflows/release.yml'),
]);

const workspaceVersion = workspace.match(/\[workspace\.package\][\s\S]*?version = "([^"]+)"/)?.[1];
const frontendManifest = JSON.parse(frontend);
const tauriConfig = JSON.parse(tauri);
const rpmVersion = spec.match(/^Version:\s*(\S+)/m)?.[1];
const appstreamVersion = metainfo.match(/<release version="([^"]+)"/)?.[1];

test('development manifests stay on one version', () => {
  assert.ok(workspaceVersion);
  assert.equal(frontendManifest.version, workspaceVersion);
  assert.equal(tauriConfig.version, workspaceVersion);
});

test('published OBS metadata describes one immutable release', () => {
  assert.ok(rpmVersion);
  assert.equal(appstreamVersion, rpmVersion);
  assert.match(spec, /^Source0:\s+%\{name\}-%\{version\}\.tar\.zst$/m);
  assert.match(spec, /^Source1:\s+vendor\.tar\.zst$/m);
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

test('Windows release starts without a console window', () => {
  assert.match(
    tauriMain,
    /^#!\[cfg_attr\(all\(windows, not\(debug_assertions\)\), windows_subsystem = "windows"\)\]$/m,
  );
});

test('tag releases publish native Windows, Debian, Fedora and openSUSE packages', async () => {
  assert.match(releaseWorkflow, /tags:\s*\n\s*- "v\[0-9\]\+\.\[0-9\]\+\.\[0-9\]\+"/);
  assert.match(releaseWorkflow, /runs-on: windows-latest/);
  assert.match(releaseWorkflow, /cargo tauri build --bundles nsis/);
  assert.match(releaseWorkflow, /cargo tauri build --bundles deb/);
  assert.match(releaseWorkflow, /container: fedora:43/);
  assert.match(releaseWorkflow, /container: opensuse\/leap:16\.0/);
  assert.equal((releaseWorkflow.match(/cargo tauri build --bundles rpm/g) ?? []).length, 2);
  assert.match(releaseWorkflow, /gh release upload/);
  assert.match(releaseWorkflow, /SHA256SUMS/);
  assert.equal(tauriConfig.bundle.windows.nsis.installMode, 'currentUser');
  assert.equal(tauriConfig.bundle.windows.webviewInstallMode.silent, true);
  assert.ok(tauriConfig.bundle.icon.includes('icons/icon.ico'));
  assert.ok(tauriConfig.bundle.linux.deb.depends.includes('xdg-desktop-portal'));
  assert.ok(tauriConfig.bundle.linux.rpm.depends.includes('xdg-desktop-portal'));
  assert.ok((await stat(new URL('../../src-tauri/icons/icon.ico', import.meta.url))).size > 0);
});

test('official RPM contains Tauri runtime dependencies without GTK fallback dependencies', () => {
  assert.match(spec, /^BuildRequires:\s+pkgconfig\(webkit2gtk-4\.1\)$/m);
  assert.match(spec, /^BuildRequires:\s+pkgconfig\(openssl\)$/m);
  assert.match(spec, /^BuildRequires:\s+pkgconfig\(librsvg-2\.0\)$/m);
  assert.match(spec, /^Requires:\s+xdg-desktop-portal$/m);
  for (const fallbackDependency of ['gtk4', 'libadwaita', 'gtksourceview-5']) {
    assert.doesNotMatch(spec, new RegExp(`pkgconfig\\(${fallbackDependency}`));
  }
});
