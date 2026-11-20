"""Imports one of *our* exported packages into a fresh collection of **real Anki** (the
official `anki` library) and checks that Anki sees exactly what we wrote (docs/10 §34).

    /tmp/ankivenv/bin/python -I tools/anki/check_import.py <package> <expect.json>

`<package>` is an `.apkg` (imported like File → Import) or a `.colpkg` (replaces the
collection, like File → Import of a collection package). `<expect.json>` is written by the
Rust test `fc-app` `real_anki::our_exports_open_in_real_anki`:

    {"notes": [{"guid", "notetype", "fields": [...], "tags": [...]}],
     "cards": [{"guid", "ord", "deck", "type", "queue", "due", "ivl", "reps", "lapses",
                "flags", "fsrs": {"stability", "difficulty"} | null}],
     "crt": s, "revlog": n, "media": {"name": "sha256 hex"}, "occlusion": ["guid", ...]}

Checks: Anki's "Check Database" finds nothing to fix; every note (by guid) has the same note
type, fields and tags; every card has the same deck, state, due, interval, counts, flag and
FSRS memory (an `.apkg` lands in an existing collection, so Anki re-bases due days on its own
creation day and appends new cards to its queue: review due dates are compared as calendar
days, new-card positions not at all); the review log has every row; every media file is in Anki's media folder with
the same bytes; every card renders (cloze cards hide their deletion); image occlusion notes
open in Anki's own occlusion editor with all their shapes. Prints a JSON report; exits 1 on
any mismatch.
"""

import hashlib
import json
import os
import shutil
import sys
import tempfile

from anki._backend import RustBackend
from anki.collection import (
    Collection,
    ImportAnkiPackageOptions,
    ImportAnkiPackageRequest,
)


def open_package(package: str, work: str) -> Collection:
    path = os.path.join(work, "collection.anki2")
    if package.endswith(".colpkg"):
        # Like Anki's File → Import of a collection: with no collection open, the backend
        # replaces the collection file, then it is opened.
        RustBackend().import_collection_package(
            col_path=path,
            backup_path=package,
            media_folder=os.path.join(work, "collection.media"),
            media_db=os.path.join(work, "collection.media.db2"),
        )
        return Collection(path)
    col = Collection(path)
    col.import_anki_package(
        ImportAnkiPackageRequest(
            package_path=package,
            options=ImportAnkiPackageOptions(
                with_scheduling=True, with_deck_configs=True, merge_notetypes=False
            ),
        )
    )
    return col


def main(package: str, expect_path: str) -> int:
    with open(expect_path, encoding="utf-8") as f:
        exp = json.load(f)
    work = tempfile.mkdtemp(prefix="fc-anki-check-")
    errors: list[str] = []
    try:
        col = open_package(os.path.abspath(package), work)

        # Integrity first: Check Database must find nothing to repair.
        problems, ok = col.fix_integrity()
        if not ok:
            errors.append(f"check database: {problems}")

        notes = {}
        for nid in col.find_notes(""):
            n = col.get_note(nid)
            notes[n.guid] = n
        if len(notes) != len(exp["notes"]):
            errors.append(f"notes: anki {len(notes)}, expected {len(exp['notes'])}")
        for en in exp["notes"]:
            n = notes.get(en["guid"])
            if n is None:
                errors.append(f"note {en['guid']} missing")
                continue
            if n.note_type()["name"] != en["notetype"]:
                errors.append(f"note {en['guid']}: type {n.note_type()['name']!r}")
            if list(n.fields) != en["fields"]:
                errors.append(f"note {en['guid']}: fields {list(n.fields)!r} != {en['fields']!r}")
            if sorted(n.tags) != sorted(en["tags"]):
                errors.append(f"note {en['guid']}: tags {n.tags!r}")

        cards = {}
        for cid in col.find_cards(""):
            c = col.get_card(cid)
            cards[(c.note().guid, c.ord)] = c
        if len(cards) != len(exp["cards"]):
            errors.append(f"cards: anki {len(cards)}, expected {len(exp['cards'])}")
        rendered = 0
        for ec in exp["cards"]:
            c = cards.get((ec["guid"], ec["ord"]))
            if c is None:
                errors.append(f"card {ec['guid']}/{ec['ord']} missing")
                continue
            got = {
                "deck": col.decks.name(c.did),
                "type": c.type,
                "queue": c.queue,
                "due": c.due,
                "ivl": c.ivl,
                "reps": c.reps,
                "lapses": c.lapses,
                "flags": c.user_flag(),
            }
            if not package.endswith(".colpkg"):
                if c.queue in (2, 3) or (c.queue < 0 and c.type == 2):
                    # Same calendar day, whatever each collection's creation day is.
                    day = (col.crt + c.due * 86400) // 86400
                    if abs(day - (exp["crt"] + ec["due"] * 86400) // 86400) <= 1:
                        got["due"] = ec["due"]
                elif c.type == 0:
                    got["due"] = ec["due"]
            for k, v in got.items():
                if ec[k] != v:
                    errors.append(f"card {ec['guid']}/{ec['ord']}: {k} anki {v!r}, expected {ec[k]!r}")
            fsrs = ec.get("fsrs")
            if fsrs is not None:
                ms = c.memory_state
                if ms is None or abs(ms.stability - fsrs["stability"]) > 1e-3 or abs(
                    ms.difficulty - fsrs["difficulty"]
                ) > 1e-3:
                    errors.append(f"card {ec['guid']}/{ec['ord']}: fsrs {ms!r} != {fsrs!r}")
            q, a = c.question(), c.answer()
            if not q.strip() or not a.strip():
                errors.append(f"card {ec['guid']}/{ec['ord']}: renders empty")
            if c.note_type()["type"] == 1 and "image-occlusion" not in q and "[...]" not in q:
                errors.append(f"card {ec['guid']}/{ec['ord']}: cloze question shows no gap: {q[:200]!r}")
            rendered += 1

        revlog = col.db.scalar("select count() from revlog")
        if revlog != exp["revlog"]:
            errors.append(f"revlog: anki {revlog}, expected {exp['revlog']}")

        media_dir = col.media.dir()
        for name, sha256 in exp["media"].items():
            p = os.path.join(media_dir, name)
            if not os.path.exists(p):
                errors.append(f"media {name} missing")
                continue
            with open(p, "rb") as f:
                if hashlib.sha256(f.read()).hexdigest() != sha256:
                    errors.append(f"media {name}: different bytes")

        shapes = 0
        for guid in exp.get("occlusion", []):
            n = notes.get(guid)
            if n is None:
                continue
            if n.note_type().get("originalStockKind") != 6:
                errors.append(f"occlusion {guid}: not Anki's image occlusion note type")
            r = col.get_image_occlusion_note(n.id)
            if r.WhichOneof("value") != "note":
                errors.append(f"occlusion {guid}: Anki cannot open it: {r}")
                continue
            shapes += len(r.note.occlusions)
            if len(r.note.occlusions) != len(n.cards()):
                errors.append(f"occlusion {guid}: {len(r.note.occlusions)} shapes, {len(n.cards())} cards")

        report = {
            "anki": __import__("anki.buildinfo").buildinfo.version,
            "package": os.path.basename(package),
            "notes": len(notes),
            "cards": len(cards),
            "rendered": rendered,
            "revlog": revlog,
            "media": len(exp["media"]),
            "occlusion_shapes": shapes,
            "errors": errors,
        }
        print(json.dumps(report, ensure_ascii=False))
        col.close()
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
