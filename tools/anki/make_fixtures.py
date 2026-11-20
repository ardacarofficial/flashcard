"""Builds small Anki packages *with the official Anki library* (pip package `anki`) for the
importer tests in `crates/fc-interop/tests/fixtures/anki/` (docs/10 §34).

Run isolated, from outside the output folder:

    python3 -m venv /tmp/ankivenv && /tmp/ankivenv/bin/pip install anki
    /tmp/ankivenv/bin/python -I tools/anki/make_fixtures.py crates/fc-interop/tests/fixtures/anki

Writes `real-latest.apkg` (Anki 2.1.50+ layout, zstd), `real-legacy.apkg` (schema 11,
`collection.anki21`), `real.colpkg` (whole collection, latest layout), `real-noscheduling.apkg`
(Anki's default "without scheduling" export) and `real.json` (what the packages contain,
read back from Anki, for the Rust tests to compare against).
"""

import json
import os
import struct
import sys
import tempfile
import zlib

from anki.collection import (
    Collection,
    ExportAnkiPackageOptions,
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


def wav(samples: int) -> bytes:
    data = bytes((i * 7) % 256 for i in range(samples))
    return (
        b"RIFF"
        + struct.pack("<I", 36 + len(data))
        + b"WAVEfmt "
        + struct.pack("<IHHIIHH", 16, 1, 1, 8000, 8000, 1, 8)
        + b"data"
        + struct.pack("<I", len(data))
        + data
    )


def stock(col: Collection, name: str):
    nt = col.models.by_name(name)
    assert nt is not None, name
    return nt


def add(col: Collection, nt, deck_id: int, fields: list[str], tags: list[str]):
    note = col.new_note(nt)
    for i, v in enumerate(fields):
        note.fields[i] = v
    note.tags = tags
    col.add_note(note, deck_id)
    return note


def main(out_dir: str) -> None:
    os.makedirs(out_dir, exist_ok=True)
    work = tempfile.mkdtemp(prefix="fc-anki-fixtures-")
    col = Collection(os.path.join(work, "collection.anki2"))
    col.set_config("fsrs", True)

    english = col.decks.id("Lang::English")
    verbs = col.decks.id("Türkçe::Fiiller")
    col.decks.save(
        {**col.decks.get(english), "desc": "Words <b>from</b> class"}
    )

    image = col.media.write_data("apple.png", png(8, 6, (200, 30, 30)))
    sound = col.media.write_data("hello.wav", wav(400))
    occl_path = os.path.join(work, "map.png")
    with open(occl_path, "wb") as f:
        f.write(png(40, 20, (30, 120, 200)))

    basic = add(col, stock(col, "Basic"), english, ["apple", f'elma <img src="{image}">'], ["fruit", "a1"])
    rev = add(col, stock(col, "Basic (and reversed card)"), english, ["book", "kitap"], ["a1"])
    typed = add(col, stock(col, "Basic (type in the answer)"), english, ["to run", "koşmak"], [])
    optional = add(col, stock(col, "Basic (optional reversed card)"), english, ["sun", "güneş", "y"], [])
    cloze = add(
        col,
        stock(col, "Cloze"),
        verbs,
        ["{{c1::Gitmek}} to go, {{c2::gelmek}} to come", "Back <i>extra</i>"],
        ["verbs"],
    )
    audio = add(col, stock(col, "Basic"), verbs, [f"hello [sound:{sound}]", "merhaba"], ["audio"])

    # A custom note type with three fields and one template.
    mm = col.models
    vocab = mm.new("Vocab TR")
    for name in ("Word", "Meaning", "Example"):
        mm.add_field(vocab, mm.new_field(name))
    t = mm.new_template("Card 1")
    t["qfmt"] = "{{Word}}"
    t["afmt"] = "{{FrontSide}}<hr id=answer>{{Meaning}}<br>{{Example}}"
    mm.add_template(vocab, t)
    mm.add(vocab)
    vocab = mm.by_name("Vocab TR")
    custom = add(col, vocab, english, ["badger", "porsuk", "The badger dug a hole."], ["animal"])

    # Image occlusion (Anki 23.10+).
    col.add_image_occlusion_notetype()
    io_nt = stock(col, "Image Occlusion")
    col.add_image_occlusion_note(
        notetype_id=io_nt["id"],
        image_path=occl_path,
        occlusions=(
            "{{c1::image-occlusion:rect:left=.1:top=.2:width=.3:height=.4:oi=1}}"
            "{{c2::image-occlusion:ellipse:left=.5:top=.1:rx=.1:ry=.15:oi=1}}"
            "{{c3::image-occlusion:polygon:points=.1,.7 .3,.9 .2,.95:oi=1}}"
        ),
        header="Map",
        back_extra="",
        tags=["io"],
    )

    # Review history and card states.
    def cards_of(note):
        return [col.get_card(cid) for cid in note.card_ids()]

    def answer(card, ease):
        card = col.get_card(card.id)
        card.start_timer()
        col.sched.answerCard(card, ease)

    c_basic = cards_of(basic)[0]
    answer(c_basic, 3)
    answer(c_basic, 3)  # graduates: review card, 1+ day interval
    c_rev_fwd, c_rev_back = cards_of(rev)
    answer(c_rev_fwd, 1)  # learning
    answer(c_rev_back, 4)  # easy: review
    col.sched.suspend_cards([cards_of(typed)[0].id])
    col.sched.bury_cards([cards_of(optional)[0].id], manual=True)
    c1, c2 = cards_of(cloze)
    col.set_user_flag_for_cards(1, [c1.id])  # red
    col.set_user_flag_for_cards(7, [c2.id])  # purple
    col.sched.set_due_date([c2.id], "5")  # review card due in 5 days
    answer(cards_of(audio)[0], 3)  # learning, one step left
    answer(cards_of(custom)[0], 4)

    def notes_json():
        out = []
        for nid in sorted(col.find_notes("")):
            n = col.get_note(nid)
            out.append(
                {
                    "guid": n.guid,
                    "notetype": n.note_type()["name"],
                    "fields": list(n.fields),
                    "tags": sorted(n.tags),
                    "cards": [
                        {
                            "ord": c.ord,
                            "deck": col.decks.name(c.did),
                            "type": c.type,
                            "queue": c.queue,
                            "ivl": c.ivl,
                            "reps": c.reps,
                            "lapses": c.lapses,
                            "flags": c.user_flag(),
                            "fsrs": None
                            if c.memory_state is None
                            else {
                                "stability": round(c.memory_state.stability, 4),
                                "difficulty": round(c.memory_state.difficulty, 4),
                            },
                        }
                        for c in sorted(
                            (col.get_card(cid) for cid in n.card_ids()), key=lambda c: c.ord
                        )
                    ],
                }
            )
        return out

    summary = {
        "anki_version": __import__("anki.buildinfo").buildinfo.version,
        "notes": notes_json(),
        "revlog": col.db.scalar("select count() from revlog"),
        "media": sorted(os.listdir(col.media.dir())),
    }

    opts = dict(with_scheduling=True, with_deck_configs=True, with_media=True)
    col.export_anki_package(
        out_path=os.path.join(out_dir, "real-latest.apkg"),
        options=ExportAnkiPackageOptions(legacy=False, **opts),
        limit=None,
    )
    col.export_anki_package(
        out_path=os.path.join(out_dir, "real-legacy.apkg"),
        options=ExportAnkiPackageOptions(legacy=True, **opts),
        limit=None,
    )
    col.export_anki_package(
        out_path=os.path.join(out_dir, "real-noscheduling.apkg"),
        options=ExportAnkiPackageOptions(
            legacy=False, with_scheduling=False, with_deck_configs=False, with_media=True
        ),
        limit=None,
    )
    col.export_collection_package(
        os.path.join(out_dir, "real.colpkg"), include_media=True, legacy=False
    )
    with open(os.path.join(out_dir, "real.json"), "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=1, sort_keys=True)
        f.write("\n")


if __name__ == "__main__":
    main(sys.argv[1])
