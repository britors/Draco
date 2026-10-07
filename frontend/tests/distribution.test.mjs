import assert from 'node:assert/strict';
import { readFile, readdir, stat } from 'node:fs/promises';
import { test } from 'node:test';

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8');
const [workspace, frontend, tauri, tauriMain, desktop, metainfo, spec, releaseSpec, releaseWorkflow] = await Promise.all([
  read('../../Cargo.toml'),
  read('../package.json'),
  read('../../src-tauri/tauri.conf.json'),
  read('../../src-tauri/src/main.rs'),
  read('../../data/br.com.dracodb.Draco.desktop'),
  read('../../data/br.com.dracodb.Draco.metainfo.xml'),
  read('../../packaging/obs/postgres-draco.spec'),
  read('../../packaging/draco-release.spec'),
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

test('installed identity is consistent across Tauri, desktop and AppStream', async () => {
  const appId = tauriConfig.identifier;
  assert.equal(appId, 'br.com.dracodb.Draco');
  assert.equal(tauriConfig.app.enableGTKAppId, true);
  assert.deepEqual(
    (await readdir(new URL('../../data/', import.meta.url))).filter((name) => name.endsWith('.desktop')),
    [`${appId}.desktop`],
  );
  assert.match(desktop, /^Exec=draco$/m);
  assert.match(desktop, new RegExp(`^Icon=${appId}$`, 'm'));
  assert.match(desktop, new RegExp(`^StartupWMClass=${appId}$`, 'm'));
  assert.match(desktop, /^Terminal=false$/m);
  assert.match(metainfo, new RegExp(`<id>${appId}</id>`));
  assert.match(metainfo, new RegExp(`<launchable type="desktop-id">${appId}\\.desktop</launchable>`));
  assert.match(metainfo, /<binary>draco<\/binary>/);
  assert.equal(tauriConfig.bundle.linux.deb.desktopTemplate, `../data/${appId}.desktop`);
  assert.equal(tauriConfig.bundle.linux.rpm.desktopTemplate, `../data/${appId}.desktop`);
  assert.match(spec, /icons\/hicolor\/\$\{size\}x\$\{size\}\/apps\/br\.com\.dracodb\.Draco\.png/);
  assert.match(spec, /icons\/hicolor\/256x256\/apps\/br\.com\.dracodb\.Draco\.png/);
  assert.match(releaseSpec, /applications\/br\.com\.dracodb\.Draco\.desktop/);
  assert.match(releaseSpec, /icons\/hicolor\/\$\{size\}x\$\{size\}\/apps\/br\.com\.dracodb\.Draco\.png/);
  assert.doesNotMatch(
    spec,
    new RegExp(`${appId.replaceAll('.', '\\.')}-symbolic`),
    'the RPM must not install a symbolic variant that GNOME can prefer over the color desktop icon',
  );
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
  assert.equal((releaseWorkflow.match(/bash scripts\/package-release-rpm\.sh/g) ?? []).length, 2);
  assert.match(releaseWorkflow, /bash scripts\/normalize-tauri-deb\.sh/);
  assert.doesNotMatch(releaseWorkflow, /applications\/Draco\.desktop/);
  assert.match(releaseWorkflow, /gh release upload/);
  assert.match(releaseWorkflow, /SHA256SUMS/);
  assert.equal(tauriConfig.bundle.windows.nsis.installMode, 'currentUser');
  assert.equal(tauriConfig.bundle.windows.webviewInstallMode.silent, true);
  assert.deepEqual(tauriConfig.bundle.icon, [
    'icons/32x32.png',
    'icons/128x128.png',
    'icons/128x128@2x.png',
    'icons/512x512.png',
    'icons/icon.ico',
  ]);
  assert.ok(tauriConfig.bundle.icon.includes('icons/icon.ico'));
  assert.ok(tauriConfig.bundle.linux.deb.depends.includes('xdg-desktop-portal'));
  assert.ok(tauriConfig.bundle.linux.rpm.depends.includes('xdg-desktop-portal'));
  assert.ok((await stat(new URL('../../src-tauri/icons/icon.ico', import.meta.url))).size > 0);
  assert.ok((await stat(new URL('../../logo-new.png', import.meta.url))).size > 0);
  for (const [file, size] of [['32x32.png', 32], ['128x128.png', 128], ['128x128@2x.png', 256], ['512x512.png', 512]]) {
    const png = await readFile(new URL(`../../src-tauri/icons/${file}`, import.meta.url));
    assert.equal(png.readUInt32BE(16), size, `${file} width`);
    assert.equal(png.readUInt32BE(20), size, `${file} height`);
  }
});

test('official RPM contains Tauri runtime dependencies without legacy GTK frontend dependencies', () => {
  assert.match(spec, /^BuildRequires:\s+pkgconfig\(webkit2gtk-4\.1\)$/m);
  assert.match(spec, /^BuildRequires:\s+pkgconfig\(openssl\)$/m);
  assert.match(spec, /^BuildRequires:\s+pkgconfig\(librsvg-2\.0\)$/m);
  assert.match(spec, /^Requires:\s+xdg-desktop-portal$/m);
  for (const legacyDependency of ['gtk4', 'libadwaita', 'gtksourceview-5']) {
    assert.doesNotMatch(spec, new RegExp(`pkgconfig\\(${legacyDependency}`));
  }
});

test('AppStream screenshots are served by the site from files kept in the repository', async () => {
  const screenshots = [...metainfo.matchAll(/<screenshot(?: type="(\w+)")?>([\s\S]*?)<\/screenshot>/g)];
  assert.ok(screenshots.length >= 4, 'software centers and Flathub expect several screenshots');
  assert.equal(screenshots.filter(([, type]) => type === 'default').length, 1);
  for (const [, , body] of screenshots) {
    assert.match(body, /<caption>[^<]+<\/caption>/);
    assert.match(body, /<caption xml:lang="pt-BR">[^<]+<\/caption>/);
    const image = body.match(/<image type="source" width="(\d+)" height="(\d+)">https:\/\/dracodb\.com\.br\/(assets\/screenshots\/[\w-]+\.png)<\/image>/);
    assert.ok(image, `unexpected screenshot image: ${body.trim()}`);
    const [, width, height, path] = image;
    const png = await readFile(new URL(`../../site/${path}`, import.meta.url));
    // PNG IHDR: width and height are big-endian u32 at offsets 16 and 20.
    assert.deepEqual([png.readUInt32BE(16), png.readUInt32BE(20)], [Number(width), Number(height)], path);
  }
});

test('the site ships matching pt-BR and English pages that the Site workflow keeps on one release', async () => {
  const [ptPage, enPage, sitemap, pagesWorkflow] = await Promise.all([
    read('../../site/index.html'),
    read('../../site/en/index.html'),
    read('../../site/sitemap.xml'),
    read('../../.github/workflows/pages.yml'),
  ]);
  const version = (page) => page.match(/"softwareVersion": "(\d+\.\d+\.\d+)"/)?.[1];
  assert.ok(version(ptPage));
  assert.equal(version(enPage), version(ptPage), 'both pages must start from the same release');
  for (const page of [ptPage, enPage]) {
    // The workflow rewrites every mention of the source version, so no other release may appear.
    const versions = new Set(page.match(/\b\d+\.\d+\.\d+\b/g));
    assert.deepEqual([...versions], [version(ptPage)]);
    assert.match(page, /<link rel="alternate" hreflang="pt-BR" href="https:\/\/dracodb\.com\.br\/">/);
    assert.match(page, /<link rel="alternate" hreflang="en" href="https:\/\/dracodb\.com\.br\/en\/">/);
    assert.match(page, /<link rel="alternate" hreflang="x-default" href="https:\/\/dracodb\.com\.br\/en\/">/);
    assert.match(page, /class="nav-lang"/, 'each page links to the other language');
  }
  assert.match(ptPage, /<html lang="pt-BR">/);
  assert.match(enPage, /<html lang="en">/);
  assert.match(enPage, /<link rel="canonical" href="https:\/\/dracodb\.com\.br\/en\/">/);
  for (const [, path] of enPage.matchAll(/(?:src|href)="\.\.\/([^"#]+)"/g)) {
    await stat(new URL(`../../site/${path}`, import.meta.url));
  }
  assert.match(sitemap, /<loc>https:\/\/dracodb\.com\.br\/en\/<\/loc>/);
  assert.match(pagesWorkflow, /for page in site\/index\.html site\/en\/index\.html; do/);
});
