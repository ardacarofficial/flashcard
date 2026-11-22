/**
 * A minimal W3C WebDriver client on Node 22's global `fetch` (no npm dependencies).
 *
 * Talks to `tauri-driver` (default http://127.0.0.1:4444), which forwards everything to
 * msedgedriver after rewriting `tauri:options` in the new-session request into
 * `ms:edgeOptions` (binary = the app's exe, browserName = webview2).
 *
 * Only what the Windows pilot needs: sessions, find/click/send keys, keyboard actions,
 * sync/async scripts, screenshots. Spec: https://www.w3.org/TR/webdriver2/
 */

/** The W3C web element identifier key. */
export const ELEMENT = 'element-6066-11e4-a52e-4f735466cecf';

/** WebDriver "normalised" key codes (spec §17.4.2) for the keys the journeys press. */
export const KEY = {
  Enter: '',
  Tab: '',
  Escape: '',
  Control: '',
  Shift: '',
  Space: ' ',
};

/** An error answered by the remote end (`{ value: { error, message, stacktrace } }`). */
export class WebDriverError extends Error {
  constructor(method, path, status, value) {
    super(`${method} ${path} → HTTP ${status} ${value?.error ?? ''}: ${value?.message ?? JSON.stringify(value)}`);
    this.status = status;
    this.code = value?.error;
  }
}

export class WebDriver {
  /**
   * @param {string} base e.g. `http://127.0.0.1:4444`
   */
  constructor(base) {
    this.base = base.replace(/\/$/, '');
    /** @type {string | null} */
    this.sessionId = null;
    /** Capabilities the driver answered with (browserVersion = the WebView2 runtime). */
    this.capabilities = null;
  }

  /** One HTTP command. Returns the `value` member of the reply. */
  async cmd(method, path, body, { timeoutMs = 120_000 } = {}) {
    const res = await fetch(`${this.base}${path}`, {
      method,
      headers: body === undefined ? {} : { 'content-type': 'application/json; charset=utf-8' },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(timeoutMs),
    });
    const text = await res.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = { value: { error: 'invalid json', message: text.slice(0, 500) } };
    }
    if (!res.ok) throw new WebDriverError(method, path, res.status, json?.value);
    return json?.value ?? null;
  }

  /** `/session/{id}` + `rest`. */
  s(rest = '') {
    if (!this.sessionId) throw new Error('no WebDriver session');
    return `/session/${this.sessionId}${rest}`;
  }

  /**
   * Starts the app through tauri-driver. Capability shape from the Tauri v2 WebDriver docs
   * (Selenium example: browserName "wry" + `tauri:options.application`); tauri-driver only
   * reads `tauri:options` from `alwaysMatch` and replaces browserName with "webview2".
   */
  async newSession(application, { args = [] } = {}) {
    const value = await this.cmd(
      'POST',
      '/session',
      {
        capabilities: {
          alwaysMatch: {
            browserName: 'wry',
            'tauri:options': { application, args },
          },
        },
      },
      { timeoutMs: 180_000 },
    );
    this.sessionId = value.sessionId;
    this.capabilities = value.capabilities ?? null;
    return value;
  }

  /** Ends the session (msedgedriver closes the app it launched). Never throws. */
  async deleteSession() {
    if (!this.sessionId) return;
    try {
      await this.cmd('DELETE', this.s(), undefined, { timeoutMs: 30_000 });
    } catch (e) {
      console.warn(`[webdriver] delete session: ${e.message}`);
    } finally {
      this.sessionId = null;
    }
  }

  setTimeouts(t) {
    return this.cmd('POST', this.s('/timeouts'), t);
  }

  getUrl() {
    return this.cmd('GET', this.s('/url'));
  }

  getTitle() {
    return this.cmd('GET', this.s('/title'));
  }

  /** First element matching a CSS selector, or null when there is none. */
  async find(css) {
    try {
      const v = await this.cmd('POST', this.s('/element'), { using: 'css selector', value: css });
      return v?.[ELEMENT] ?? null;
    } catch (e) {
      if (e.code === 'no such element') return null;
      throw e;
    }
  }

  /** The focused element's id. */
  async active() {
    const v = await this.cmd('GET', this.s('/element/active'));
    return v?.[ELEMENT] ?? null;
  }

  click(el) {
    return this.cmd('POST', this.s(`/element/${el}/click`), {});
  }

  /** Element Send Keys: focuses the element and types `text` into it. */
  sendKeys(el, text) {
    return this.cmd('POST', this.s(`/element/${el}/value`), { text });
  }

  /**
   * Keyboard input as real (trusted) key events. `keys` is a list of chords; a chord is an
   * array of key values pressed in order and released in reverse (`[[KEY.Control, KEY.Enter]]`),
   * a plain string is one key (`' '`, `'2'`, `'n'`).
   */
  async press(...keys) {
    const actions = [];
    for (const k of keys) {
      const chord = Array.isArray(k) ? k : [k];
      for (const v of chord) actions.push({ type: 'keyDown', value: v });
      for (const v of [...chord].reverse()) actions.push({ type: 'keyUp', value: v });
    }
    await this.cmd('POST', this.s('/actions'), { actions: [{ type: 'key', id: 'keyboard', actions }] });
    // Release whatever might still be held (spec: Release Actions).
    await this.cmd('DELETE', this.s('/actions'));
  }

  /** Execute Script (sync). `fn` is a function serialized into the page; args must be JSON. */
  exec(fn, ...args) {
    return this.cmd('POST', this.s('/execute/sync'), {
      script: `return (${fn.toString()}).apply(null, arguments);`,
      args,
    });
  }

  /**
   * Execute Async Script: `fn` may return a promise; its value comes back, a rejection is
   * thrown here with the page's error (an `ApiError` of the app is JSON-stringified).
   */
  async execAsync(fn, ...args) {
    const script = `
      const done = arguments[arguments.length - 1];
      const args = Array.prototype.slice.call(arguments, 0, -1);
      Promise.resolve()
        .then(() => (${fn.toString()}).apply(null, args))
        .then(
          (value) => done({ ok: true, value: value === undefined ? null : value }),
          (e) => done({ ok: false, error: e instanceof Error ? String(e.stack || e) : JSON.stringify(e) }),
        );`;
    const r = await this.cmd('POST', this.s('/execute/async'), { script, args });
    if (!r?.ok) throw new Error(`page script failed: ${r?.error ?? JSON.stringify(r)}`);
    return r.value;
  }

  /** PNG screenshot of the window, as a Buffer. */
  async screenshot() {
    const b64 = await this.cmd('GET', this.s('/screenshot'));
    return Buffer.from(b64, 'base64');
  }
}
