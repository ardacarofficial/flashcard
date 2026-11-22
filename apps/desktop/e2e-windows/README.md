# Windows WebDriver pilot (v1.3)

A **pilot**, not a gate: three journeys of the Linux suite (`../e2e`) run against the
**installed** Windows app (NSIS, per-user, WebView2) through
[`tauri-driver`](https://v2.tauri.app/develop/tests/webdriver/) and a `msedgedriver` that
matches the WebView2 runtime. It runs in the `webdriver-pilot` job of
`.github/workflows/windows-build.yml` (`continue-on-error: true`, after the installer job).

**Status: written without a Windows machine; untested until CI runs it.** Expect the first
runs to need fixes (driver/runtime match, focus, selectors).

| Step | What | Asserted on |
|---|---|---|
| P | First-run guide skipped (`onboarding.skip`), a 5-note Classic deck and an empty deck created through the app's own IPC, page reloaded | IPC replies |
| J1 | The app launches, the shell and Home (`home-start`) render | DOM |
| J2 | Quick Add: `N`, type front, `Tab`, type back, `Ctrl+Enter` (as J1 in `../e2e`) | `notes.list` / `notes.get`: one note, exact fields, chosen deck |
| J3 | Home → `Enter` → launcher → `Enter` → `Space`×3 → `2` (as S1 in `../e2e`) | `study.session`: exactly one answered; `study.card_history`: one review |

No npm dependencies: `webdriver.mjs` is a small W3C WebDriver client on Node 22's `fetch`;
`run.mjs` starts tauri-driver, creates the session (`tauri:options.application` = the exe),
runs the journeys, writes `summary.json`, screenshots (and the DOM of a failed step) and
`tauri-driver.log` to the output dir, prints a `PASS`/`FAIL` line per journey, exits 1 when
anything failed, and always stops tauri-driver and the app. IPC calls go through
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
`e2e-windows/.results`), `--port` (`FC_WIN_E2E_PORT`, default 4444; msedgedriver gets port+1).
