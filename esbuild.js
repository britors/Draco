const esbuild = require('esbuild');
const fs = require('fs');
const path = require('path');

const isWatch = process.argv.includes('--watch');
const isProd  = process.argv.includes('--production');

const shared = {
  bundle: true,
  format: 'cjs',
  platform: 'node',
  sourcemap: !isProd,
  minify: isProd,
  external: ['electron', 'ssh2', 'cpu-features'],
};

function copyAssets() {
  fs.mkdirSync('out/renderer', { recursive: true });

  // Renderer HTML
  fs.copyFileSync('src/renderer/index.html', 'out/renderer/index.html');

  // DDL panel HTML
  if (fs.existsSync('src/renderer/ddl.html')) {
    fs.copyFileSync('src/renderer/ddl.html', 'out/renderer/ddl.html');
  }

  // About HTML
  if (fs.existsSync('src/renderer/about.html')) {
    fs.copyFileSync('src/renderer/about.html', 'out/renderer/about.html');
  }

  // Codicons (local copy so no CDN dependency)
  const codiDir = path.join('node_modules', '@vscode', 'codicons', 'dist');
  if (fs.existsSync(codiDir)) {
    fs.copyFileSync(path.join(codiDir, 'codicon.css'), 'out/renderer/codicon.css');
    fs.copyFileSync(path.join(codiDir, 'codicon.ttf'), 'out/renderer/codicon.ttf');
  }
}

async function main() {
  copyAssets();

  const mainCtx = await esbuild.context({
    ...shared,
    entryPoints: ['src/main/index.ts'],
    outfile: 'out/main/index.js',
  });

  const preloadCtx = await esbuild.context({
    ...shared,
    entryPoints: ['src/preload/index.ts'],
    outfile: 'out/preload/index.js',
  });

  if (isWatch) {
    await mainCtx.watch();
    await preloadCtx.watch();
    console.log('Watching for changes…');
  } else {
    await mainCtx.rebuild();
    await preloadCtx.rebuild();
    await mainCtx.dispose();
    await preloadCtx.dispose();
    console.log('Build complete.');
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
