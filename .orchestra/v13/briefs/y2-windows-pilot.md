# Y2 — Windows WebDriver pilot: make the session start (or prove why it cannot)

Owner: `apps/desktop/e2e-windows/**`, the `webdriver-pilot` job in `.github/workflows/windows-build.yml`,
its docs. Do not touch the `installer` job except where stated. You cannot run Windows here: think carefully,
make the job self-diagnosing, the orchestrator pushes and reads the CI log.

## What happened on the first CI run (run 45038113867, job "WebDriver pilot (non-gating)")
Installer built and installed fine. Versions: WebView2 157.0.4341.46, msedgedriver matched, tauri-driver 2.1.0.
```
[e2e-windows] starting ...tauri-driver.exe --port 4444 --native-port 4445 --native-driver D:\a\_temp\msedgedriver\msedgedriver.exe
[e2e-windows] creating a session for C:\Users\runneradmin\AppData\Local\Flashcard\fc-desktop.exe
(60 s later)
FAIL setup Error: POST /session → HTTP 500 session not created: session not created: DevToolsActivePort file doesn't exist
    at WebDriver.newSession (apps/desktop/e2e-windows/webdriver.mjs:77)  ← run.mjs:492
FAIL windows pilot: no journeys ran
```
So msedgedriver launched the app but never saw the WebView2 DevTools port.

## Do
1. Find the likely causes by reading our code and the Tauri v2 WebDriver docs/tauri-driver source
   (look in ~/.cargo/registry if present, else reason from the docs): e.g. capabilities shape
   (`tauri:options.application`, `browserName: "wry"`, `ms:edgeOptions`/`webviewOptions`), the app exiting
   early when launched by the driver (single-instance plugin, a first-run path, a missing data dir, the app
   needing an env var), the WebView2 user data folder, the app not honouring
   `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` / `--remote-debugging-port`, the release build's CSP or
   devtools feature. Check `apps/desktop/src-tauri` (main/lib, tauri.conf, plugins, Cargo features) for
   anything that makes the app quit or not create its window in that situation and fix what is really wrong.
2. Make the job diagnose itself so one CI run tells us the truth:
   - before the journeys: start the installed exe on its own for ~15 s, report whether the process is still
     alive, its window title, and any log/crash output; then kill it;
   - on session failure: dump tauri-driver and msedgedriver output (run msedgedriver with `--verbose` /
     log file), the process list (fc-desktop, msedgewebview2), and upload all of it in the results artifact;
   - retry session creation a couple of times with a shorter timeout.
3. If the diagnosis from reading is confident, apply the fix. If two candidate fixes are plausible, make
   run.mjs try them in order (e.g. different capability shapes) and log which one worked.
4. The job stays `continue-on-error`. Update the docs line that describes the pilot's status honestly.

## Checks
YAML parses, `node --check` on every changed .mjs, `cargo fmt/clippy` if Rust changed
(`--workspace --exclude fc-desktop` compiles here; fc-desktop itself does not), `pnpm -C apps/desktop check`
if TS changed. Report plainly that nothing here ran on Windows.
