# J2: installed-app journeys on Windows, and the Subtitle check that cannot silently skip

Read `j-common.md`. Two CI jobs, both verified only by CI (I, the orchestrator, push your branch's commits to the
main branch and send you the failing logs; write the code so the first run has a real chance).

## A. Windows installed-app journeys (`apps/desktop/e2e-windows/run.mjs`, `.github/workflows/windows-build.yml`)

Today the pilot runs P and J1–J3 against the installed app through WebDriver, as non-blocking work. Read it and its
README first, then add the journeys the reviewer asked for, each checking the saved result by reading it back
(through the app or the local API, as the existing journeys do):

1. **Capture**: with the app running, a Subtitle capture sent to the local API (the same request the extension
   sends: pairing, then a word with its sentence and media) shows up in the app as a card with its encounter.
2. **Reopen**: close the app, start it again; the card, the review done before closing and the encounter are there.
3. **Backup and restore**: make a backup from the app, change something, restore, the change is gone and the
   earlier state is back.
4. **Rescue**: stop the app, damage the collection file (truncate it), start the app: the rescue dialog's choice
   leads to a working collection restored from the verified backup. The dialog is a native one (rfd): if WebDriver
   cannot drive it, find how the code can be driven in a test without weakening the product (for example the
   existing `fc rescue` path for the choice plus the installed app for the result) and say plainly what is and is
   not covered. Do not add a hidden test-only switch to the release build without saying so in the report.

Keep each journey independent and its failure message useful (what was expected, what was read back, screenshot).
Decide with reasons whether the new journeys block the build: a journey that is stable should block, one that
depends on a flaky native piece stays non-blocking and is named as such. Update `apps/desktop/e2e-windows/README.md`.

## B. Subtitle client check in `ci.yml` (around line 223)

The job checks out `ardacarofficial/subtitle` with a token; when the checkout is unavailable it prints a warning
and exits 0, so a release can go out without the real client ever being checked. Make that impossible to miss
without making every fork or token-less run red for no reason:
- On pushes to this repository's branches where the secret is expected: missing checkout = failed job, with a
  message that says which secret is missing and how to set it.
- Where the secret legitimately cannot exist (pull requests from forks): skipped, shown as skipped, not as success.
- First find out from the workflow and its history what actually happens today on `claude/festive-einstein-8y0kld`
  (does the checkout succeed?). If the secret is absent today, making the job fail would turn CI red until the
  owner adds it: in that case keep the job red-capable behind one clearly named repository variable, leave it off,
  make the job's summary say in a visible line "Subtitle client NOT checked", and put the exact steps for the owner
  in the report. Do not guess; read the last CI logs' lines if you can, otherwise state the uncertainty.

Local checks: `node --check` on the scripts, a YAML parse of both workflows, and run whatever part of `run.mjs`
can run here against `fc dev serve` or `fc api serve` (the capture request and the read-back at least).
