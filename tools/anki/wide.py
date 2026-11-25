"""Wider real-Anki fixtures and Anki's own report of a package (docs/16-anki-uyum.md).

Built *with the official Anki library* (pip package `anki`, the version CI pins). Run isolated,
from outside the output folder:

    python -I tools/anki/wide.py make <out_dir> [--large N]
    python -I tools/anki/wide.py report <package> <report.json>
    python -I tools/anki/wide.py compare <anki.json> <roundtrip.json>

`make` builds two collections and exports each with scheduling, deck options and media:

* `wide-fsrs.apkg` (latest layout, zstd, scheduler v3, FSRS on): two templates per note type,
  cloze with three gaps, sub-decks three deep, hierarchical tags (`lang::en::verb`), media
  with non-ASCII names (`şarkı ğ.wav`, `çiçek.png`), suspended / buried (by hand) / flagged
  cards, a filtered deck holding two cards, "set due date" and "forget" entries in the review
  log next to normal answers, and a `--large` review history (N cards × 8 answers);
* `wide-sm2.apkg` (legacy schema 11, `collection.anki21`, FSRS off): SM-2 ease factors
  after Hard / Easy answers, a lapse, a relearning card, a deck with its own options preset.

Next to each package, `<name>.json` is what Anki itself reports about that collection
(`report` format below). `report` imports any package into a fresh collection exactly like
File → Import and prints the same report, so Anki → fc → Anki can be compared with `compare`
(prints the list of differences as JSON, one string each).

Report: {"notes": {note id: {guid, notetype, fields, tags}}, "cards": {"note id/ord": {deck,
filtered, type, queue, due_in, ivl, factor, reps, lapses, flags, fsrs, revlog: [[type, ease,
ivl], ...]}}, "revlog_total", "media": {name: sha256}, "decks": [...], "fsrs": bool}.
`due_in` is days from Anki's today (review / day-learning) or seconds from now rounded to the
minute (intraday learning); `null` for new cards.
"""

import hashlib
import json
import os
import random
import struct
import sys
import tempfile
import time
import zlib

from anki.collection import (
    Collection,
    ExportAnkiPackageOptions,
    ImportAnkiPackageOptions,
    ImportAnkiPackageRequest,
)


def png(width: int, height: int, rgb: tuple[int, int, int]) -> bytes:
    raw = b"".join(b"\x00" + bytes(rgb) * width for _ in range(height))

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def wav(samples: int, seed: int) -> bytes:
    data = bytes((i * seed) % 256 for i in range(samples))
    return (
        b"RIFF"
        + struct.pack("<I", 36 + len(data))
        + b"WAVEfmt "
        + struct.pack("<IHHIIHH", 16, 1, 1, 8000, 8000, 1, 8)
        + b"data"
        + struct.pack("<I", len(data))
        + data
    )


def add(col: Collection, nt_name: str, deck_id: int, fields: list[str], tags: list[str]):
    note = col.new_note(col.models.by_name(nt_name))
    for i, v in enumerate(fields):
        note.fields[i] = v
    note.tags = tags
    col.add_note(note, deck_id)
    return note


def answer(col: Collection, cid: int, ease: int) -> None:
    card = col.get_card(cid)
    card.start_timer()
    col.sched.answerCard(card, ease)


def build_fsrs(col: Collection, large: int) -> None:
    col.set_config("fsrs", True)
    deep = col.decks.id("Dil::İngilizce::Fiiller")
    other = col.decks.id("Dil::Almanca")
    img = col.media.write_data("çiçek.png", png(6, 4, (10, 160, 40)))
    snd = col.media.write_data("şarkı ğ.wav", wav(300, 11))

    # Two templates per note type: a custom type with a forward and a reverse card.
    mm = col.models
    vt = mm.new("Kelime 2 yön")
    for name in ("Kelime", "Anlam", "Ses"):
        mm.add_field(vt, mm.new_field(name))
    for name, q, a in (
        ("İleri", "{{Kelime}}", "{{FrontSide}}<hr id=answer>{{Anlam}} {{Ses}}"),
        ("Geri", "{{Anlam}}", "{{FrontSide}}<hr id=answer>{{Kelime}}"),
    ):
        t = mm.new_template(name)
        t["qfmt"], t["afmt"] = q, a
        mm.add_template(vt, t)
    mm.add(vt)

    a = add(col, "Kelime 2 yön", deep, ["go", f'gitmek <img src="{img}">', f"[sound:{snd}]"], ["lang::en::verb", "a1"])
    b = add(col, "Kelime 2 yön", deep, ["take", "almak", ""], ["lang::en::verb"])
    c = add(col, "Cloze", other, ["{{c1::Hund}} köpek, {{c2::Katze}} kedi, {{c3::Maus}} fare", ""], ["lang::de"])
    d = add(col, "Basic", other, ["Baum", "ağaç"], ["lang::de", "orman::ağaç"])
    e = add(col, "Basic", deep, ["run", "koşmak"], [])
    f = add(col, "Basic", deep, ["walk", "yürümek"], [])
    g = add(col, "Basic", deep, ["see", "görmek"], [])

    a1, a2 = a.card_ids()
    b1, b2 = b.card_ids()
    c1, c2, c3 = c.card_ids()
    (d1,) = d.card_ids()
    (e1,) = e.card_ids()
    (f1,) = f.card_ids()
    (g1,) = g.card_ids()

    for cid, eases in ((a1, [3, 3]), (a2, [4]), (b1, [1]), (c1, [3, 3, 3]), (d1, [4]), (e1, [4]), (f1, [4]), (g1, [4])):
        for ease in eases:
            answer(col, cid, ease)
    answer(col, c1, 1)  # lapse: relearning
    # "Set due date" on a review card and on a new one; "forget" on a reviewed one.
    col.sched.set_due_date([e1], "12")
    col.sched.set_due_date([c2], "3")
    col.sched.schedule_cards_as_new([f1], restore_position=True, reset_counts=False)
    col.sched.suspend_cards([b2])
    col.sched.bury_cards([c3], manual=True)
    col.set_user_flag_for_cards(2, [a1])  # orange
    col.set_user_flag_for_cards(4, [d1])  # blue

    # A filtered deck holding the two review cards `a1` and `g1` (their home stays "Fiiller").
    fd = col.sched.get_or_create_filtered_deck(0)
    fd.name = "Filtre::Bugün"
    fd.config.search_terms[0].search = f"cid:{a1},{g1}"
    fd.config.search_terms[0].limit = 10
    fd.config.reschedule = True
    col.sched.add_or_update_filtered_deck(fd)

    if large:
        bulk = col.decks.id("Dil::Büyük")
        rng = random.Random(7)
        for i in range(large):
            n = add(col, "Basic", bulk, [f"w{i}", f"k{i}"], ["büyük"])
            cid = n.card_ids()[0]
            for _ in range(8):
                answer(col, cid, rng.choice([1, 2, 3, 3, 3, 4]))


def build_sm2(col: Collection) -> None:
    col.set_config("fsrs", False)
    deck = col.decks.id("SM2::Kelimeler")
    conf = col.decks.add_config_returning_id("Yavaş")
    cfg = col.decks.get_config(conf)
    cfg["new"]["delays"] = [1.0, 10.0, 60.0]
    cfg["rev"]["perDay"] = 77
    col.decks.update_config(cfg)
    d = col.decks.get(deck)
    d["conf"] = conf
    col.decks.save(d)

    notes = [add(col, "Basic (and reversed card)", deck, [f"word{i}", f"kelime{i}"], ["sm2"]) for i in range(3)]
    (x1, x2), (y1, y2), (z1, _z2) = (n.card_ids() for n in notes)
    for cid, eases in ((x1, [4]), (x2, [3, 3, 3, 2]), (y1, [4, 4]), (y2, [3, 3, 1]), (z1, [3, 3, 4])):
        for ease in eases:
            answer(col, cid, ease)
    col.sched.set_due_date([y1], "20!")  # "!" also resets the interval


def card_report(col: Collection, c, now: float, today: int) -> dict:
    if c.type == 0:
        due_in = None
    elif c.queue == 1 or (c.queue in (-1, -2, -3) and c.due > 1_000_000_000):
        due_in = round((c.due - now) / 60) * 60
    else:
        due_raw = c.odue if c.odid else c.due
        due_in = due_raw - today
    revlog = col.db.all(f"select type, ease, ivl from revlog where cid = {c.id} order by id")
    return {
        "deck": col.decks.name(c.odid or c.did),
        "filtered": col.decks.name(c.did) if c.odid else None,
        "type": c.type,
        "queue": c.queue,
        "due_in": due_in,
        "ivl": c.ivl,
        "factor": c.factor,
        "reps": c.reps,
        "lapses": c.lapses,
        "flags": c.user_flag(),
        "fsrs": None
        if c.memory_state is None
        else {
            "stability": round(c.memory_state.stability, 3),
            "difficulty": round(c.memory_state.difficulty, 3),
        },
        "revlog": [list(r) for r in revlog],
    }


def report(col: Collection) -> dict:
    now = time.time()
    today = col.sched.today
    notes, cards = {}, {}
    for nid in col.find_notes(""):
        n = col.get_note(nid)
        notes[str(n.id)] = {
            "guid": n.guid,
            "notetype": n.note_type()["name"],
            "fields": list(n.fields),
            "tags": sorted(n.tags),
        }
        for cid in n.card_ids():
            c = col.get_card(cid)
            cards[f"{n.id}/{c.ord}"] = card_report(col, c, now, today)
    media = {}
    for name in sorted(os.listdir(col.media.dir())):
        with open(os.path.join(col.media.dir(), name), "rb") as f:
            media[name] = hashlib.sha256(f.read()).hexdigest()
    return {
        "anki_version": __import__("anki.buildinfo").buildinfo.version,
        "fsrs": bool(col.get_config("fsrs", False)),
        "notes": notes,
        "cards": cards,
        "revlog_total": col.db.scalar("select count() from revlog"),
        "media": media,
        "decks": sorted(d.name for d in col.decks.all_names_and_ids() if d.name != "Default"),
    }


def write_json(path: str, value) -> None:
    with open(path, "w", encoding="utf-8") as f:
        json.dump(value, f, ensure_ascii=False, indent=1, sort_keys=True)
        f.write("\n")


def make(out_dir: str, large: int) -> None:
    os.makedirs(out_dir, exist_ok=True)
    for name, legacy, build in (
        ("wide-fsrs", False, lambda col: build_fsrs(col, large)),
        ("wide-sm2", True, build_sm2),
    ):
        work = tempfile.mkdtemp(prefix=f"fc-{name}-")
        col = Collection(os.path.join(work, "collection.anki2"))
        build(col)
        problems, ok = col.fix_integrity()
        assert ok, problems
        col.export_anki_package(
            out_path=os.path.join(out_dir, f"{name}.apkg"),
            options=ExportAnkiPackageOptions(
                legacy=legacy, with_scheduling=True, with_deck_configs=True, with_media=True
            ),
            limit=None,
        )
        # What Anki reports about the package it wrote (imported back the same way as ours).
        write_json(os.path.join(out_dir, f"{name}.json"), import_report(os.path.join(out_dir, f"{name}.apkg")))
        col.close()


def import_report(package: str) -> dict:
    work = tempfile.mkdtemp(prefix="fc-anki-report-")
    col = Collection(os.path.join(work, "collection.anki2"))
    col.import_anki_package(
        ImportAnkiPackageRequest(
            package_path=os.path.abspath(package),
            options=ImportAnkiPackageOptions(
                with_scheduling=True, with_deck_configs=True, merge_notetypes=False
            ),
        )
    )
    problems, ok = col.fix_integrity()
    out = report(col)
    out["check_database"] = [] if ok else list(problems)
    col.close()
    return out


def compare(a: dict, b: dict) -> list[str]:
    """Every difference between Anki's report of its own package (`a`) and of our export of
    the same collection (`b`)."""
    diffs = []
    # Keyed by note id: Anki keeps the ids of an imported package, and so do we (a note's
    # creation time); the guid is compared as a field.
    for nid, n in a["notes"].items():
        m = b["notes"].get(nid)
        if m is None:
            diffs.append(f"note {nid}: missing")
            continue
        for k in ("guid", "notetype", "fields", "tags"):
            if n[k] != m[k]:
                diffs.append(f"note {nid}: {k} {n[k]!r} -> {m[k]!r}")
    for key, c in a["cards"].items():
        d = b["cards"].get(key)
        if d is None:
            diffs.append(f"card {key}: missing")
            continue
        for k in ("deck", "filtered", "type", "queue", "due_in", "ivl", "factor", "reps", "lapses", "flags", "fsrs"):
            x, y = c[k], d[k]
            if k == "due_in" and isinstance(x, int) and isinstance(y, int) and abs(x) > 1000:
                same = abs(x - y) <= 120  # intraday learning: seconds, two minutes apart at most
            else:
                same = x == y
            if not same:
                diffs.append(f"card {key}: {k} {x!r} -> {y!r}")
        if [r[:2] for r in c["revlog"]] != [r[:2] for r in d["revlog"]]:
            diffs.append(f"card {key}: revlog {c['revlog']!r} -> {d['revlog']!r}")
    for name, sha in a["media"].items():
        if b["media"].get(name) != sha:
            diffs.append(f"media {name}: {'missing' if name not in b['media'] else 'bytes differ'}")
    if a["revlog_total"] != b["revlog_total"]:
        diffs.append(f"revlog_total {a['revlog_total']} -> {b['revlog_total']}")
    for d in sorted(set(a["decks"]) - set(b["decks"])):
        diffs.append(f"deck {d}: missing")
    if b.get("check_database"):
        diffs.append(f"check database: {b['check_database']}")
    return diffs


def main(argv: list[str]) -> int:
    cmd = argv[1]
    if cmd == "make":
        large = int(argv[argv.index("--large") + 1]) if "--large" in argv else 0
        make(argv[2], large)
    elif cmd == "report":
        write_json(argv[3], import_report(argv[2]))
    elif cmd == "compare":
        with open(argv[2], encoding="utf-8") as f:
            a = json.load(f)
        with open(argv[3], encoding="utf-8") as f:
            b = json.load(f)
        print(json.dumps(compare(a, b), ensure_ascii=False, indent=1))
    else:
        raise SystemExit(f"unknown command {cmd}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
