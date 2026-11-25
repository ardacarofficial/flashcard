# Windows journeys against the installed app (v1.3 pilot → v1.4.2 gate)

Journeys of the Linux suite (`../e2e`) and the owner's data journeys run against the
**installed** Windows app (NSIS, per-user, WebView2) through
[`tauri-driver`](https://v2.tauri.app/develop/tests/webdriver/) and a `msedgedriver` that
matches the WebView2 runtime. They run in the `webdriver-pilot` job ("Windows journeys
(installed app)") of `.github/workflows/windows-build.yml`, after the installer job.

**Status.** P and J1–J3 have been green on every run since the session fix below (11 runs in a
row up to 45206222145; session via `driver+policy` in ~1.5 s). v1.4.2 adds J4–J7 and makes the
job a **gate**: it fails when a *blocking* journey fails. J4–J7 were written and their requests
and read-backs checked on Linux (`local-check.mjs`, below); **their first Windows run is the next
CI run.**

## Which journeys block, and why

| Journey | Blocks? | Why |
|---|---|---|
| P, J1–J3 | yes | Green on every run since the session strategy was fixed |
| J4 capture, J5 reopen, J6 backup/restore | yes | Only the same machinery as J1–J3: the WebDriver session, the app's IPC, a loopback HTTP request, an app restart through the same session strategy. No native UI |
| J7 rescue | **no** (warning) | Answers a **native Win32 message box** from outside the app and damages files of a killed app: the flaky native piece |

The list is one line in the workflow (`FC_WIN_E2E_BLOCKING: P,J1,J2,J3,J4,J5,J6`; `--blocking`
locally). A failed session or setup always fails the job. A non-blocking failure prints a
`::warning` and `WARNING: non-blocking J7 failed` on the final line and the run's summary page.

## What J7 covers, and what it does not

The rescue dialog is shown by the shell **before any window** (`src-tauri/src/session.rs` →
`maintenance.rs::offer_rescue`) with rfd 0.15 without `common-controls-v6`, i.e. a plain
`MessageBoxW`: the three labels become **Yes / No / Cancel**, and Yes = "restore the newest
verified backup". WebDriver cannot see it (no webview yet). J7 answers it the way a click does,
from outside: while msedgedriver waits for the webview, `win.mjs::answerAppDialog` finds the
app's `#32770` window, takes a desktop screenshot (`j7-damaged-dialog-1.png`), posts
`WM_COMMAND IDYES`, then `IDOK` to the "rescued" box. **No test switch was added to the app**;
the release build is unchanged. The PASS line names the path taken (`rescued by: …`):

| `rescued_by` | Covered | Not covered |
|---|---|---|
| `native dialog: Yes …` | the real dialog, the real choice, the shell's rescue, the result in the installed app | a human click (a posted `WM_COMMAND` is what the button sends, not a mouse) |
| `in-app rescue …` | the truncated file still opened; its integrity check failed; Maintenance → `maintenance.rescue.request` → restart → rescue | the native dialog (not shown in that case) |
| `FALLBACK fc rescue --from backup …` | the same `rescue()` through the installed `fc.exe`, the result in the installed app | **the dialog**: it was not answered or the app did not come up after it |

Before damaging, J7 makes a verified backup in `backups/`, writes one more note, stops the
app, folds the write-ahead log into the file with the installed `fc.exe` (so the damage is not
masked by pages still waiting in the log), and truncates `collection.db` to 2/5. After: the
backup's notes are back, the later note is gone, `damaged/<time>/collection.db` is the
truncated file byte-for-byte in size, the full integrity check passes, a new note is read back.

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
| J4 | Subtitle module on, an access key made (`subtitle.keys.create`, Settings › Subtitle); the extension's pairing (`GET /subtitle/v1/me` with the key), picture + sound (`POST /v1/media`), the word with its sentence (`POST /v1/captures`, `Idempotency-Key`) to the **app's** local API; the resend | `201 created` / `200 duplicate` with the same card; `notes.get`: the note owns that card, word highlighted in the sentence, two media fields; `subtitle.encounters.list`: the capture is the primary encounter, video and time kept |
| J5 | A capture into its own deck, one review (`study.start` / `study.answer good`), the app **closed** (WM_CLOSE, then forced) and **started again** | after the restart: the same note/card/encounter (as J4), `study.card_history`: exactly that one review (rating, session) |
| J6 | Two notes, `io.backup.create` (the job), then a note added and one edited, `io.backup.restore` (safety backup into the results), page reloaded | the deck holds exactly the backup's two notes, the edit is undone, the safety backup exists |
| J7 | See above: verified backup, a later note, app stopped, file truncated, app started, rescue answered | the backup's state, the later note gone, the damaged file kept, integrity check OK, a new write read back |

Every journey makes its own decks, key and capture, so one failing does not take the next with
it; a journey that finds no session (a restart before it failed) starts the app again first.
On failure: what was expected and what was read back, a screenshot and the DOM
(`<id>-fail.png/html`), and `summary.json` → `journeys[].facts`.

`data-journeys.mjs` holds J4–J7 without WebDriver: they get the app's API, "close/start the
app" and "start on a damaged collection" from the runner. `local-check.mjs` runs them on any
machine against `fc dev serve` (same dispatch as the IPC) + `fc api serve` (the same HTTP server
the app runs) on a temporary collection; there "start on a damaged collection" is
`fc rescue --from backup` (no native dialog off Windows):

```bash
cargo build -p fc-cli
node apps/desktop/e2e-windows/local-check.mjs            # J4–J7; --only J4,J6
```

No npm dependencies: `webdriver.mjs` is a small W3C WebDriver client on Node 22's `fetch`;
`win.mjs` holds the Windows helpers (policy, processes, windows, desktop screenshot, CDP
probe, answering the app's native message box; PowerShell/`reg.exe`, never throwing); `run.mjs` runs the preflight, gets a session
(above), runs the journeys, writes `summary.json`, screenshots (and the DOM of a failed
step) and the logs to the output dir, prints a `PASS`/`FAIL` line per journey (also on the
workflow run's summary page), exits 1 when a blocking journey (or the setup) failed, and always stops tauri-driver
and the app. Step P also dismisses the crash-recovery offer a forced stop leaves behind
(`maintenance.recovery.dismiss`), so it cannot cover Home. IPC calls go through
`window.__TAURI_INTERNALS__.invoke('invoke', { command: 'v1.<method>', payload })`, the same
call the frontend's transport makes. Selectors are the `data-testid`s / roles the Linux
journeys use; no visible text is matched (the UI language follows the OS).

## Run locally (Windows)

The app always opens the real per-user profile (`%APPDATA%\app.flashcard.desktop`) and the
pilot adds decks to it (and J7 truncates its collection file, J6 restores a backup over it). **Use a throwaway Windows user or VM**; `run.mjs` refuses to start
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

Options (or env): `--app` (`FC_EXE`), `--fc` (`FC_CLI`, default `fc.exe` beside the app),
`--blocking P,J1,…` (`FC_WIN_E2E_BLOCKING`), `--only J4,J6` (`FC_WIN_E2E_ONLY`: those + P/J1),
`--native-driver` (`MSEDGEDRIVER`), `--tauri-driver`
(`TAURI_DRIVER`, default `tauri-driver` on `PATH`), `--out` (`FC_WIN_E2E_OUT`, default
`e2e-windows/.results`), `--port` (`FC_WIN_E2E_PORT`, default 4444; msedgedriver gets port+1),
`--cdp-port` (`FC_WIN_E2E_CDP_PORT`, default 9333), `--strategy driver+policy|attach+policy|driver`
(`FC_WIN_E2E_STRATEGY`: only that one), `--no-verbose-driver` (`FC_WIN_E2E_VERBOSE_DRIVER=0`).

The policy goes to HKLM when the shell is elevated, else to HKCU (honoured by non-elevated
hosts; may be read-only for a standard user — the attempt then records the error and the
`driver` strategy still runs). Whatever was there before is put back at the end.
