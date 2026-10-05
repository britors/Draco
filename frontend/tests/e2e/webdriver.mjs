// Minimal W3C WebDriver client shared by the installed-app E2E and the screenshot capture.
//
// It talks plain HTTP to `tauri-driver` so the frontend keeps zero runtime dependencies.
const DRIVER = process.env.DRACO_E2E_DRIVER_URL || 'http://127.0.0.1:4444';
const APPLICATION = process.env.DRACO_E2E_APP || '/usr/bin/draco';
const ELEMENT = 'element-6066-11e4-a52e-4f735466cecf';
const TIMEOUT_MS = Number(process.env.DRACO_E2E_TIMEOUT_MS || 30000);

async function webdriver(method, path, body) {
  const response = await fetch(`${DRIVER}${path}`, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    const error = payload?.value?.error || response.status;
    const message = payload?.value?.message || response.statusText;
    throw new Error(`WebDriver ${method} ${path} failed: ${error} ${message}`);
  }
  return payload.value;
}

export class Session {
  static async start() {
    const value = await webdriver('POST', '/session', {
      capabilities: { alwaysMatch: { 'tauri:options': { application: APPLICATION } } },
    });
    return new Session(value.sessionId);
  }

  constructor(id) { this.id = id; }

  command(method, path, body) { return webdriver(method, `/session/${this.id}${path}`, body); }

  async quit() { await webdriver('DELETE', `/session/${this.id}`).catch(() => {}); }

  // Runs a synchronous script in the webview and returns its JSON-serializable result.
  run(script, ...args) { return this.command('POST', '/execute/sync', { script, args }); }

  async find(css) {
    const value = await this.command('POST', '/element', { using: 'css selector', value: css });
    return value[ELEMENT];
  }

  // WebKitWebDriver rejects native pointer and keyboard input on some sessions (notably Wayland),
  // so interactions go through DOM events. The app's own listeners still handle every action.
  async click(css) {
    await this.find(css);
    await this.run('document.querySelector(arguments[0]).click();', css);
  }

  async type(css, text) {
    await this.find(css);
    await this.run(`
      const [selector, text] = arguments;
      const field = document.querySelector(selector);
      field.focus();
      field.value = text;
      field.dispatchEvent(new Event('input', { bubbles: true }));
    `, css, text);
  }

  // Polls `script` until it returns a truthy value; the last value is reported on timeout.
  async waitFor(description, script, ...args) {
    const deadline = Date.now() + TIMEOUT_MS;
    let last;
    while (Date.now() < deadline) {
      last = await this.run(script, ...args).catch((error) => `error: ${error.message}`);
      if (last && !String(last).startsWith('error:')) return last;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`Timed out waiting for ${description} (last: ${JSON.stringify(last)})`);
  }

  // Captures the webview as PNG bytes. The window has no native decorations, so this is the
  // whole application surface.
  async screenshot() {
    return Buffer.from(await this.command('GET', '/screenshot'), 'base64');
  }
}
