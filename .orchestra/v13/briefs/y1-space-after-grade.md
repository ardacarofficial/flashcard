# Y1 — a key pressed right after a grade never lands on the old card

Owner: `apps/desktop/src/features/study/**`, `apps/desktop/e2e/**` (new journey only). No Rust changes expected.

## Problem (left open by the closing pass)
In a Cards session, when the user grades a card (digit) and presses Space immediately after, the UI applies
that Space to the card that was just graded (it is still on screen while the answer is being saved), instead of
to the next card. The owner's central complaint about v1.2 was "one key does two jobs"; this is the last
remnant of it. The S4 journey was made to wait for the next card to avoid this; the product must not need that.

## Wanted behaviour
- Between a grade being accepted and the next card's front being shown, the card on screen is "leaving":
  Space, digits and H do nothing to it and are NOT queued for the next card either (a key press acts only on
  what the user can see). Z (undo) and Esc keep their current, already-tested behaviour (Esc pauses once the
  answer is saved; undo undoes the grade).
- No visible flicker: the graded card must not flip back to its front.
- Same rule for the other study surfaces that grade and advance with keys, if they share the weakness
  (check Learn/Test/Match quickly; fix only where the same bug exists).

## Acceptance
- Unit tests for the key handling in the leaving state.
- A new journey on the real core (dev bridge) that grades and presses Space with no wait in between, several
  cards in a row, then verifies from the API that each card got exactly one review with the pressed grade and
  that the card now on screen shows its FRONT. Do not weaken S4; you may remove its extra wait if the fix makes
  it unnecessary (then run e2e three times in a row).
- `pnpm -C apps/desktop check`, `test`, `e2e` green; docs/11 key table and e2e README journey table updated
  if a journey is added.
