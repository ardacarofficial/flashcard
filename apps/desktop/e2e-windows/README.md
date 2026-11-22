# Windows WebDriver pilot (v1.3)

A **pilot**, not a gate: three journeys of the Linux suite (`../e2e`) run against the
**installed** Windows app (NSIS, per-user, WebView2) through
[`tauri-driver`](https://v2.tauri.app/develop/tests/webdriver/) and a `msedgedriver` that
matches the WebView2 runtime. It runs in the `webdriver-pilot` job of
`.github/workflows/windows-build.yml` (`continue-on-error: true`, after the installer job).

**Status: not green yet.** The first CI run (45038113867) installed the app and matched the
driver (WebView2 157.0.4341.46, tauri-driver 2.1.0) but could not create a session:
`session not created: DevToolsActivePort file doesn't exist`. The likely cause and the fix
below were worked out by reading (the tauri-driver 2.1.0 / wry / Tauri sources and public
WebView2 reports); **nothing here has run on Windows since.** The next run says which
session strategy worked (or why none did); the journeys themselves have never run yet.

## Why the session failed, and what the pilot does about it

tauri-driver on Windows only rewrites `tauri:options` into `ms:edgeOptions` (`binary` = the
exe, browserName `webview2`) and starts msedgedriver. msedgedriver starts the app and asks
WebView2 for a DevTools port through the `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` environment
variable, then waits 60 s for the browser's `DevToolsActivePort` file. **Since WebView2
Runtime 150 an elevated host ignores that variable** (and the HKCU policy) — Microsoft calls
it intended (WebView2Feedback #5640/#5645; same symptom in runner-images #14738 on
windows-2025). GitHub's Windows runners run elevated, so the port never opens. What an
elevated host still honours: the **HKLM** policy
`HKLM\SOFTWARE\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments`, value
`<exe name>` = `--remote-debugging-port=…`, or arguments the app itself passes through the
WebView2 API.

Nothing in the app was found to quit or skip its window under a driver (single instance only
matters if a copy is already running; data folders are created; the CSP does not apply to
CDP; the release build only disables the DevTools *window*, which should not close the CDP
port — the preflight checks exactly that). So the fix lives in the pilot, and the
shipped app is unchanged:

1. **PREFLIGHT** — the installed exe runs alone for 15 s with the HKLM policy opening port
   9333: is it alive, which windows does it show (class `#32770` = an error box), did WebView2
   open the port (`/json/version`)? Then it is closed (politely first, so the next start
   is clean). One line in the log: `PREFLIGHT app alive after 15s, window "Flashcard" […],
   DevTools port 9333 open after 3.2s`.
2. **SESSION** — strategies in order, each retried, the first that works is used and logged
   (`SESSION via "…"`, `summary.json` → `strategy`, `attempts`):

   | Strategy | How | Tries |
   |---|---|---|
   | `driver+policy` | HKLM policy `--remote-debugging-port=0`; msedgedriver launches the app (the normal tauri-driver path) | 2 |
   | `attach+policy` | HKLM policy with port 9333; the pilot starts the app, waits for the port, msedgedriver attaches by `debuggerAddress` (no `DevToolsActivePort` file involved) | 2 |
   | `driver` | no policy: exactly the first run's setup (works for non-elevated hosts on older runtimes) | 1 |

   Before every try the app (single instance) and its WebView2 processes are stopped. The
   policy value is removed again at the end (and by the workflow's last step).
3. **Evidence** — every failed try writes `<try>-diagnosis.json` and `<try>-desktop.png`
   (and `<try>-during-*` 20 s into a launch): app/WebView2/driver processes with command
   lines (does the WebView2 browser process carry `--remote-debugging-port`, which
   `--user-data-dir`?), their windows, `DevToolsActivePort` files, the CDP endpoint.
   msedgedriver runs through a small `.cmd` wrapper with `--verbose --log-path`
   (`msedgedriver-<try>.log`); tauri-driver's output is `tauri-driver-<try>.log`; the
   app's own log and crash reports are copied to `app-profile/`.

If `driver+policy` and `attach+policy` both fail with the port open, the remaining options
are an app-side switch that passes `--remote-debugging-port` through the WebView2 API in a
test build, or tauri's embedded WebDriver plugin — both change what is tested, so neither is
done blindly.

## Journeys

| Step | What | Asserted on |
|---|---|---|
| P | First-run guide skipped (`onboarding.skip`), a 5-note Classic deck and an empty deck created through the app's own IPC, page reloaded | IPC replies |
| J1 | The app launches, the shell and Home (`home-start`) render | DOM |
| J2 | Quick Add: `N`, type front, `Tab`, type back, `Ctrl+Enter` (as J1 in `../e2e`) | `notes.list` / `notes.get`: one note, exact fields, chosen deck |
| J3 | Home → `Enter` → launcher → `Enter` → `Space`×3 → `2` (as S1 in `../e2e`) | `study.session`: exactly one answered; `study.card_history`: one review |

No npm dependencies: `webdriver.mjs` is a small W3C WebDriver client on Node 22's `fetch`;
`win.mjs` holds the Windows helpers (policy, processes, windows, desktop screenshot, CDP
probe; PowerShell/`reg.exe`, never throwing); `run.mjs` runs the preflight, gets a session
(above), runs the journeys, writes `summary.json`, screenshots (and the DOM of a failed
step) and the logs to the output dir, prints a `PASS`/`FAIL` line per journey (also on the
workflow run's summary page), exits 1 when anything failed, and always stops tauri-driver
and the app. Step P also dismisses the crash-recovery offer a forced stop leaves behind
(`maintenance.recovery.dismiss`), so it cannot cover Home. IPC calls go through
`window.__TAURI_INTERNALS__.invoke('invoke', { command: 'v1.<method>', payload })`, the same
call the frontend's transport makes. Selectors are the `data-testid`s / roles the Linux
journeys use; no visible text is matched (the UI language follows the OS).

## Run locally (Windows)

The app always opens the real per-user profile (`%APPDATA%\app.flashcard.desktop`) and the
pilot adds decks to it. **Use a throwaway Windows user or VM**; `run.mjs` refuses to start
when that folder exists (outside CI) unless `FC_WIN_E2E_ALLOW_EXISTING_PROFILE=1`. Close any
running Flashcard first (single instance).

```powershell
cargo install tauri-driver --locked
# WebView2 runtime version → the exact msedgedriver build
$v = (Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}').pv
Invoke-WebRequest "https://msedgedriver.microsoft.com/$v/edgedriver_win64.zip" -OutFile edgedriver.zip
Expand-Archive edgedriver.zip -DestinationPath .\msedgedriver
node apps/desktop/e2e-windows/run.mjs --app "$env:LOCALAPPDATA\Flashcard\<app>.exe" `
  --native-driver .\msedgedriver\msedgedriver.exe
```

Options (or env): `--app` (`FC_EXE`), `--native-driver` (`MSEDGEDRIVER`), `--tauri-driver`
(`TAURI_DRIVER`, default `tauri-driver` on `PATH`), `--out` (`FC_WIN_E2E_OUT`, default
`e2e-windows/.results`), `--port` (`FC_WIN_E2E_PORT`, default 4444; msedgedriver gets port+1),
`--cdp-port` (`FC_WIN_E2E_CDP_PORT`, default 9333), `--strategy driver+policy|attach+policy|driver`
(`FC_WIN_E2E_STRATEGY`: only that one), `--no-verbose-driver` (`FC_WIN_E2E_VERBOSE_DRIVER=0`).

The policy goes to HKLM when the shell is elevated, else to HKCU (honoured by non-elevated
hosts; may be read-only for a standard user — the attempt then records the error and the
`driver` strategy still runs). Whatever was there before is put back at the end.
