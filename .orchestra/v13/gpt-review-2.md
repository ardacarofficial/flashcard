## 1. Verification

I’d adopt your split. Linux should run the full journey suite. Windows should answer three questions that Linux cannot settle. Reuse the installed NSIS artifact and keep the existing `--smoke` as a cheap diagnostic. [Current CI](/home/user/flashcard/.github/workflows/ci.yml:250)

`tauri-driver` on GitHub Actions is realistic. Stability on your exact runner is unverified. Tauri documents Windows CI, but a `windows-2025-vs2026` runner report reproduced session-creation failure even with a matching driver. Pilot on your `windows-2025` label, log the image, WebView2 runtime and driver versions, and promote the job to a release gate after repeated clean launches. I checked the sources and runner reports read-only. I did not run Windows. [Tauri CI guide](https://v2.tauri.app/develop/tests/webdriver/ci/) · [Runner report](https://github.com/actions/runner-images/issues/14738)

My minimum Windows journeys:

- Fresh install → create an English deck → Quick Add → `Ctrl+Enter` → verify the word appears through the normal event path.
- Home → Study → flip twice → grade → close and reopen → Continue. Assert zero reviews after flipping, exactly one after grading, and the same session after restart.
- Open and cancel a native import picker, verify keyboard focus returns, then import a CSV from a Unicode path and open the resulting card. Native-dialog interaction needs OS-level automation or an explicit manual check.

The Linux bridge should share desktop dispatch, lock gating, serialized errors and post-commit events. The existing HTTP API has extension-origin and actor restrictions. Give the test harness an isolated temporary profile and authenticated loopback endpoint. Keep step counts as diagnostics. Persisted outcomes are the acceptance criteria. [Desktop dispatch](/home/user/flashcard/apps/desktop/src-tauri/src/ipc.rs:26) · [HTTP boundary](/home/user/flashcard/crates/fc-http/src/lib.rs:4)

## 2. Field roles

A field describes **what it contains**. A template determines **what this card asks**. Your sketch needs both. A forward vocabulary card expects `meaning`; its reverse expects `term`. Add `surface` for *went* versus *go*. Media already has a field kind. Let a template optionally name its answer field by stable key, with typed, choice and cloze engines retaining authority. Mask the captured occurrence in the example and check that the prompt itself doesn’t disclose the missing word. [Field and template definitions](/home/user/flashcard/crates/fc-core/src/notetype.rs:41)

Fields and templates are stored as JSON. Optional metadata can deserialize old records without SQL DDL. Seeding only inserts missing types, so changing seeds alone misses the owner’s collection. Use a tested, idempotent data migration for known built-in definitions; preserve customized templates. For unmapped user types, retain existing grading, show actual field names, and open the full editor when “Definition” is ambiguous. Never guess from translated labels. [Storage](/home/user/flashcard/crates/fc-storage/src/sqlite/notetypes.rs:21) · [Seeding](/home/user/flashcard/crates/fc-app/src/support/content.rs:217)

## 3. Keyboard

Enter should activate the **focused control**. With focus on the revealed card, it has no grading shortcut. Space toggles, digits grade, and holding a key produces one action. One-handed review already works with thumb on Space and fingers on `1`/`2`. Keep arrows for navigation and text editing. The existing shortcut editor can accommodate another preference if the owner actually requests it. Skip an “Anki-style” setting.

## 4. Launcher

Use one keyboard-accessible dialog. **Study** opens it; **Test** opens it with Test selected. I retract my earlier bypass recommendation: a bare “Test” button conceals whether the remembered scope means three missed words or the whole deck. Remember scope per mode, show deck and eligible count on Start, and put **Continue English · 7 answered** first when applicable. Revalidate on Start. An empty replacement must preserve the unfinished session. [Current replacement behavior](/home/user/flashcard/crates/fc-app/src/modules/study/session.rs:249)

## 5. Re-study

Both scopes should be practice-only. Define “struggled today” visibly, initially as distinct cards rated Again during the local study day. Freeze that set when practice starts. A missed card that becomes due again legitimately affects scheduling through **Review due**. That gives the owner a clear route to each outcome.

## 6. Design refinement

I overstated the case against a design pass. Spend the day on three visible changes: enlarge settings descriptions from `text-xs` to body size and stack long controls earlier; keep the primary deck action visible before hover; give meaning, example sentence and source distinct typographic levels. Check Turkish at 900 px with keyboard focus visible. Keep dense numeric tables. [FormRow](/home/user/flashcard/apps/desktop/src/lib/features/core/kit/FormRow.svelte) · [ListRow](/home/user/flashcard/apps/desktop/src/lib/ui/ListRow.svelte)

## 7. One-day cut

Land in this order, with verification running alongside implementation:

1. Fix double-Space grading and disabled-module dead clicks. Start one Linux bridge journey and the Windows launch pilot.
2. Share Rust queue eligibility with Home and the launcher. Reproduce the 21-card, 20-limit case; preserve waiting and resumable sessions.
3. Fix built-in subtitle grading and definition editing together. Verify *went → gitmek* through editing, hints and a written test.

Defer the full role editor, twelve-journey matrix, provider gallery, import reorganization, tour and Planning simulator. Planning’s current estimate varies retention; a new-cards-per-day forecast needs its own Rust model. [PlanningScreen](/home/user/flashcard/apps/desktop/src/lib/features/stats/PlanningScreen.svelte:112)

- **Changed my mind on:** Linux-first verification, preselected mode buttons, bounded visual refinement.
- **Still disagree on:** twelve journeys before fixes, hidden remembered scope, and field roles alone deciding grading.