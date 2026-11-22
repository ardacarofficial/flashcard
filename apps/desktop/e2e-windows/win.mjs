/**
 * Windows helpers for the WebDriver pilot (no npm dependencies): the WebView2 registry
 * policy that opens the DevTools port, process and window lists, a desktop screenshot,
 * stopping the app, and probing the DevTools (CDP) endpoint.
 *
 * Every helper is safe to call anywhere: off Windows, or when a tool fails, it returns
 * `null`/an error string instead of throwing — diagnostics must never end the run.
 *
 * Why the policy (see ./README.md, "Why the session failed"): msedgedriver asks a WebView2
 * app for its DevTools port through the `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` environment
 * variable. Since WebView2 Runtime 150 an ELEVATED host ignores that variable (and the HKCU
 * policy); GitHub's Windows runners run elevated, so WebView2 never opens the port and
 * msedgedriver fails with "DevToolsActivePort file doesn't exist". What an elevated host still
 * honours is the HKLM policy `SOFTWARE\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments`
 * (value name = the app's exe name), or arguments the app passes through the WebView2 API.
 */
import { spawnSync } from 'node:child_process';

export const IS_WINDOWS = process.platform === 'win32';

/** WebView2 policy key; a value named after the exe adds browser arguments for that app. */
export const POLICY_KEY = 'SOFTWARE\\Policies\\Microsoft\\Edge\\WebView2\\AdditionalBrowserArguments';

/** The processes the pilot reports on. */
const WATCHED = ['msedgewebview2.exe', 'msedgedriver.exe', 'tauri-driver.exe'];

/**
 * Runs a Windows PowerShell script (`-EncodedCommand`: no quoting problems). Inputs go in
 * through `env` (read as `$env:NAME`), never spliced into the script.
 * @returns {{ code: number | null, out: string, err: string }}
 */
export function ps(script, env = {}, timeoutMs = 60_000) {
  if (!IS_WINDOWS) return { code: null, out: '', err: 'not windows' };
  const encoded = Buffer.from(`$ErrorActionPreference = 'Stop'\n${script}`, 'utf16le').toString('base64');
  const r = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', encoded], {
    encoding: 'utf8',
    timeout: timeoutMs,
    env: { ...process.env, ...env },
    windowsHide: true,
  });
  return { code: r.status, out: (r.stdout ?? '').trim(), err: `${r.stderr ?? ''}${r.error ? ` ${r.error.message}` : ''}`.trim() };
}

/** `reg.exe` with plain arguments. */
function reg(...args) {
  const r = spawnSync('reg.exe', args, { encoding: 'utf8', timeout: 30_000, windowsHide: true });
  return { code: r.status, out: (r.stdout ?? '').trim(), err: `${r.stderr ?? ''}${r.error ? ` ${r.error.message}` : ''}`.trim() };
}

/** Whether this process runs elevated (admin token). `null` when unknown / not Windows. */
export function isElevated() {
  const r = ps(
    '([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)',
  );
  if (r.code !== 0) return null;
  return /true/i.test(r.out);
}

/**
 * The `AdditionalBrowserArguments` policy value for one exe, in HKLM (honoured by elevated
 * hosts) or, when HKLM is not writable (a non-admin local run), HKCU (honoured by
 * non-elevated hosts). Remembers what was there before and puts it back in `restore()`.
 */
export class WebView2Policy {
  /** @param {string} exeName e.g. `fc-desktop.exe` */
  constructor(exeName) {
    this.exe = exeName;
    /** hive → the value before the pilot touched it (`null` = there was none). */
    this.saved = new Map();
    /** What is set now: `{ hive, value }` or null. */
    this.current = null;
  }

  static read(hive, exe) {
    const r = reg('query', `${hive}\\${POLICY_KEY}`, '/v', exe);
    if (r.code !== 0) return null;
    const m = r.out.split(/\r?\n/).map((l) => l.match(/\s+REG_(?:EXPAND_)?SZ\s+(.*)$/)).find(Boolean);
    return m ? m[1].trim() : '';
  }

  remember(hive) {
    if (!this.saved.has(hive)) this.saved.set(hive, WebView2Policy.read(hive, this.exe));
  }

  write(hive, value) {
    this.remember(hive);
    const r = value === null ? reg('delete', `${hive}\\${POLICY_KEY}`, '/v', this.exe, '/f') : reg('add', `${hive}\\${POLICY_KEY}`, '/v', this.exe, '/t', 'REG_SZ', '/d', value, '/f');
    return r.code === 0 ? null : (r.err || r.out || `reg exited ${r.code}`);
  }

  /**
   * Sets `value` (e.g. `--remote-debugging-port=0`) for the exe, HKLM first. `null` clears
   * what the pilot set. Returns `{ hive, value, readback, errors }`.
   */
  set(value) {
    if (!IS_WINDOWS) return { hive: null, value, readback: null, errors: ['not windows'] };
    if (value === null) {
      this.restore();
      return { hive: null, value: null, readback: null, errors: [] };
    }
    const errors = [];
    for (const hive of ['HKLM', 'HKCU']) {
      const e = this.write(hive, value);
      if (e) {
        errors.push(`${hive}: ${e}`);
        continue;
      }
      // Only one hive at a time: two copies would pass the switch twice.
      const other = hive === 'HKLM' ? 'HKCU' : 'HKLM';
      if (this.saved.has(other)) this.put(other, this.saved.get(other));
      this.current = { hive, value };
      return { hive, value, readback: WebView2Policy.read(hive, this.exe), errors };
    }
    this.current = null;
    return { hive: null, value, readback: null, errors };
  }

  /** Puts `original` back in `hive` (null = no value). */
  put(hive, original) {
    const e = original === null ? (WebView2Policy.read(hive, this.exe) === null ? null : this.write(hive, null)) : this.write(hive, original);
    if (e) console.warn(`[e2e-windows] could not restore the WebView2 policy in ${hive}: ${e}`);
  }

  /** Everything back as it was before the pilot. Never throws. */
  restore() {
    for (const [hive, original] of this.saved) this.put(hive, original);
    this.current = null;
  }
}

/**
 * The processes of the app, WebView2, msedgedriver and tauri-driver, with command lines (the
 * WebView2 browser process's command line shows whether `--remote-debugging-port` and which
 * `--user-data-dir` it got). `null` when the list cannot be taken.
 */
export function processes(appExeName) {
  const r = ps(
    `$names = @($env:FC_PS_NAMES -split ',')
     $p = @(Get-CimInstance Win32_Process | Where-Object { $names -contains $_.Name.ToLowerInvariant() } |
       Select-Object ProcessId, ParentProcessId, Name, CommandLine)
     ConvertTo-Json -InputObject $p -Depth 3 -Compress`,
    { FC_PS_NAMES: [appExeName, ...WATCHED].filter(Boolean).map((n) => n.toLowerCase()).join(',') },
  );
  if (r.code !== 0) return { error: r.err || r.out };
  try {
    const v = JSON.parse(r.out || '[]');
    return Array.isArray(v) ? v : [v];
  } catch {
    return { error: `unparsable: ${r.out.slice(0, 500)}` };
  }
}

/**
 * Top-level windows: every window of the given pids, plus every visible titled window of
 * any process (an error box, a WebView2 installer, a UAC prompt…). Class `#32770` is a
 * dialog box; Tauri's main window has a tao class.
 */
export function windows(pids = []) {
  const r = ps(
    `Add-Type -TypeDefinition @"
using System; using System.Text; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class FcPilotWin {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public static List<string> All() {
    var r = new List<string>();
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      var t = new StringBuilder(512); GetWindowText(h, t, 512);
      var c = new StringBuilder(256); GetClassName(h, c, 256);
      r.Add(pid + "\\t" + IsWindowVisible(h) + "\\t" + c + "\\t" + t);
      return true;
    }, IntPtr.Zero);
    return r;
  }
}
"@
[FcPilotWin]::All() | ForEach-Object { $_ }`,
  );
  if (r.code !== 0) return { error: r.err || r.out };
  const want = new Set(pids.map(Number));
  return r.out
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => {
      const [pid, visible, cls, ...title] = line.split('\t');
      return { pid: Number(pid), visible: visible === 'True', class: cls, title: title.join('\t') };
    })
    .filter((w) => want.has(w.pid) || (w.visible && w.title));
}

/** PNG of the whole (virtual) desktop into `file`. Returns null or the error. */
export function desktopScreenshot(file) {
  const r = ps(
    `Add-Type -AssemblyName System.Windows.Forms, System.Drawing
     $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
     $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
     $g = [System.Drawing.Graphics]::FromImage($bmp)
     $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
     $bmp.Save($env:FC_PS_FILE, [System.Drawing.Imaging.ImageFormat]::Png)
     $g.Dispose(); $bmp.Dispose()`,
    { FC_PS_FILE: file },
  );
  return r.code === 0 ? null : r.err || r.out || `powershell exited ${r.code}`;
}

/** Whether any process with this image name runs. */
export function running(imageName) {
  if (!IS_WINDOWS) return false;
  const r = spawnSync('tasklist.exe', ['/FI', `IMAGENAME eq ${imageName}`, '/NH', '/FO', 'CSV'], { encoding: 'utf8', windowsHide: true });
  return (r.stdout ?? '').toLowerCase().includes(`"${imageName.toLowerCase()}"`);
}

const pause = (ms) => new Promise((ok) => setTimeout(ok, ms));

/**
 * Stops every instance of the app: first politely (WM_CLOSE, so the run ends cleanly and the
 * next start offers no crash recovery), then by force. The app is single-instance: a leftover
 * one would swallow the next launch (it forwards its arguments and exits). Also stops the
 * WebView2 processes of the app's profile (`marker` in their command line) — a surviving
 * browser process is reused by the next launch WITHOUT the new arguments — and, on CI, every
 * msedgewebview2.exe. Returns what it did.
 */
export async function stopApp(appExeName, { marker = null, ci = false } = {}) {
  if (!IS_WINDOWS || !appExeName) return 'not windows';
  const did = [];
  if (running(appExeName)) {
    spawnSync('taskkill.exe', ['/IM', appExeName, '/T'], { stdio: 'ignore', windowsHide: true });
    did.push('WM_CLOSE');
    for (let i = 0; i < 16 && running(appExeName); i++) await pause(500);
    if (running(appExeName)) {
      spawnSync('taskkill.exe', ['/F', '/IM', appExeName, '/T'], { stdio: 'ignore', windowsHide: true });
      did.push('killed');
      for (let i = 0; i < 10 && running(appExeName); i++) await pause(500);
    }
    did.push(running(appExeName) ? 'STILL RUNNING' : 'stopped');
  }
  if (ci) {
    if (running('msedgewebview2.exe')) {
      spawnSync('taskkill.exe', ['/F', '/IM', 'msedgewebview2.exe', '/T'], { stdio: 'ignore', windowsHide: true });
      did.push('msedgewebview2 killed');
    }
  } else if (marker) {
    const r = ps(
      `Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" |
         Where-Object { $_.CommandLine -and $_.CommandLine.Contains($env:FC_PS_MARKER) } |
         ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue; $_.ProcessId }`,
      { FC_PS_MARKER: marker },
    );
    if (r.out) did.push(`msedgewebview2 of the profile killed (${r.out.split(/\s+/).length})`);
  }
  // The WebView2 processes take a moment to go after their host.
  for (let i = 0; i < 10 && ci && running('msedgewebview2.exe'); i++) await pause(500);
  return did.join(', ') || 'nothing running';
}

/**
 * The DevTools endpoint on 127.0.0.1:port: `/json/version` and the page targets, or why not.
 * @returns {Promise<{ ok: boolean, version?: any, targets?: any[], error?: string }>}
 */
export async function cdp(port, timeoutMs = 2_000) {
  try {
    const get = async (path) => {
      const res = await fetch(`http://127.0.0.1:${port}${path}`, { signal: AbortSignal.timeout(timeoutMs) });
      if (!res.ok) throw new Error(`${path}: HTTP ${res.status}`);
      return res.json();
    };
    const version = await get('/json/version');
    const targets = (await get('/json/list').catch(() => [])).map((t) => ({ type: t.type, url: t.url, title: t.title }));
    return { ok: true, version, targets };
  } catch (e) {
    return { ok: false, error: e?.cause?.code ?? e?.message ?? String(e) };
  }
}
