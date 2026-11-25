"""Anki's own backend on the perf suite's collection shape (docs/12-performans.md, "Anki ile karşılaştırma").

Builds, with the official `anki` Python package, a collection shaped like the one
`crates/fc-app/src/perf_tests.rs` builds (100k two-card notes / 200k cards / 2M review rows,
the same decks, tags, state mix and query set), then times Anki's backend calls that do
what our suite times. Prints one JSON object of medians (ms) at the end.

    python -I tools/anki/bench.py --dir <scratch dir> [--notes 100000 --reviews 2000000] [--runs 5]
    python -I tools/anki/bench.py --dir <scratch dir> --reuse     # measure an existing build again

Not a correctness test. Classifications (our `level` / `exam` attributes) become tags
(`level::B1`, `exam::TYT`): Anki has no typed classification. Notes, cards and the revlog
are bulk-inserted with SQL in one transaction (the build is not what is measured), then
`check database` makes Anki register the tags and validate the rows.

What the numbers are: each operation is run `--runs` times and its median reported; the
answer loop reports p50 / p95 of 40 answers per run. `peak_rss_mb` is the whole Python
process (interpreter, the 200k-id lists of the searches) and, without `--reuse`, also the
build: read it from a `--reuse` run. Anki's backend commits lazily (answers stay in an open
transaction), so "answer" is not a durable write as ours is. Run it a few times: on a
shared machine single runs differ by 10-50 %.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import resource
import shutil
import statistics
import sys
import time

from anki.collection import Collection, DeckIdLimit, ExportAnkiPackageOptions, ImportAnkiPackageOptions, ImportAnkiPackageRequest
from anki.import_export_pb2 import CsvMetadata, CsvMetadataRequest
from anki.scheduler_pb2 import CardAnswer
from anki.cards import Card  # after anki.collection (circular import)

DAY_MS = 86_400_000

# Same words as perf_tests.rs (Turkish letters on purpose).
WORDS = [
    "kılıç", "kalkan", "ağaç", "çiçek", "göl", "şehir", "ışık", "öğretmen", "apple", "badger",
    "river", "mountain", "journey", "whisper", "harbor", "lantern", "meadow", "thunder", "türev",
    "integral", "kuvvet", "enerji", "hücre", "atom",
]
LEVELS = ["A1", "A2", "B1", "B2", "C1", "C2"]  # fc-core CEFR template
EXAMS = ["TYT", "AYT", "YDT"]  # fc-core YKS template

# perf_tests.rs SEARCH_QUERIES -> the same question in Anki's search syntax.
B1_UP = "(" + " OR ".join(f"tag:level::{lv}" for lv in LEVELS[2:]) + ")"
SEARCHES = [
    ("", ""),
    ("kılıç", "kılıç"),
    ('"ağaç çiçek"', '"ağaç çiçek"'),
    ("deck:İngilizce is:due", "deck:İngilizce is:due"),
    ("tag:fiil -is:suspended", "tag:fiil -is:suspended"),
    ("level>=B1", B1_UP),
    ("level:B1 is:due", "tag:level::B1 is:due"),
    ("exam:TYT deck:YKS", "tag:exam::TYT deck:YKS"),
    ("(level:A1 OR level:C2) apple", "(tag:level::A1 OR tag:level::C2) apple"),
    ("-level:A1 deck:İngilizce tag:isim", "-tag:level::A1 deck:İngilizce tag:isim"),
    ("stability>20 lapses>=2", "prop:s>20 prop:lapses>=2"),
    ("retrievability<0.8", "prop:r<0.8"),
    ("front:wh*", "front:wh*"),
    ("is:new deck:YKS::TYT", "is:new deck:YKS::TYT"),
]
# Browser sort orders: ours -> Anki's browser column.
SORTS = [("sort_field", "noteFld"), ("due", "cardDue"), ("modified", "cardMod")]


def ms(t0: float) -> float:
    return (time.perf_counter() - t0) * 1000.0


def median(xs: list[float]) -> float:
    return round(statistics.median(xs), 1)


def p95(xs: list[float]) -> float:
    s = sorted(xs)
    return round(s[max(0, min(len(s), -(-len(s) * 95 // 100)) - 1)], 1)


def build(path: str, notes: int, reviews: int) -> None:
    """The perf suite's world (perf_tests.rs `build_world`) as an Anki collection."""
    rng = random.Random(2026)
    roll = rng.randrange
    col = Collection(path)
    col.set_config("fsrs", True)  # FSRS on, as in our app
    col.set_config("sortType", "noteFld")
    english = col.decks.id("İngilizce")
    yks = col.decks.id("YKS")
    en_decks = [col.decks.id(f"İngilizce::Kelimeler {i}") for i in range(1, 9)]
    yks_decks = []
    for exam in ["TYT", "AYT"]:
        yks_decks.append(col.decks.id(f"YKS::{exam}"))
        for lesson in ["Matematik", "Fizik", "Kimya"]:
            yks_decks.append(col.decks.id(f"YKS::{exam}::{lesson}"))
    assert english and yks
    nt = col.models.by_name("Basic (and reversed card)")
    mid = nt["id"]
    now_ms = int(time.time() * 1000)
    now_s = now_ms // 1000
    today = col.sched.today
    seed_tags = ["fiil", "isim", "fiil::düzensiz"]

    note_rows, card_rows, review_cards = [], [], []
    base_nid = now_ms - 800 * DAY_MS  # unique millisecond ids before anything the app makes
    for i in range(notes):
        front = f"{WORDS[roll(len(WORDS))]} {WORDS[roll(len(WORDS))]} {i}"
        back = f"{WORDS[roll(len(WORDS))]} — örnek cümle {i}"
        tags = [t for t in seed_tags if roll(3) == 0]
        english_note = roll(3) != 0
        did = en_decks[roll(len(en_decks))] if english_note else yks_decks[roll(len(yks_decks))]
        if roll(5) != 0:
            tags.append(f"level::{LEVELS[roll(6)]}" if english_note else f"exam::{EXAMS[roll(3)]}")
        created = now_ms - (1 + roll(700)) * DAY_MS
        mod = (created + roll(30) * DAY_MS) // 1000
        nid = base_nid + i
        csum = int(hashlib.sha1(front.encode()).hexdigest()[:8], 16)
        tag_str = f" {' '.join(tags)} " if tags else ""
        note_rows.append((nid, f"b{i:x}g", mid, mod, -1, tag_str, f"{front}\x1f{back}", front, csum, 0, ""))
        for k in range(2):
            cid = base_nid + notes + 2 * i + k
            r = roll(20)
            data = ""
            if r <= 4:  # 25 % new
                typ, queue, due, ivl, reps, lapses, left = 0, 0, i, 0, 0, 0, 0
            elif r <= 6:  # 10 % (re)learning, due within the hour
                relearn = roll(2) == 1
                typ = 3 if relearn else 1
                queue, due, ivl = 1, now_s - 3600 + roll(7200), (5 if relearn else 0)
                s, d = 0.5 + roll(30) / 10.0, 1.0 + roll(90) / 10.0
                reps, lapses, left = 1 + roll(3), roll(3), 1
                data = json.dumps({"s": s, "d": d, "dr": 0.9, "lrt": now_s - 86_400})
            else:  # 65 % review, ~1/12 due today or overdue
                typ, queue = 2, 2
                due = today + roll(120) - 10
                s, d = 1.0 + roll(3000) / 10.0, 1.0 + roll(90) / 10.0
                reps, lapses, left = 2 + roll(20), roll(8), 0
                ivl = max(1, int(s))
                data = json.dumps({"s": s, "d": d, "dr": 0.9, "lrt": now_s - (1 + roll(60)) * 86_400})
                review_cards.append(cid)
            if roll(40) == 0:
                queue = -1
            flags = 1 if roll(25) == 0 else 0
            card_rows.append((cid, nid, did, k, mod, -1, typ, queue, due, ivl, 2500 if typ else 0,
                              reps, lapses, left, 0, 0, flags, data))
    span = 730 * DAY_MS
    rev_rows = []
    for n in range(reviews):
        at = now_ms - span + (n * span) // max(1, reviews)
        again = roll(10) == 0
        rev_rows.append((at, review_cards[roll(len(review_cards))], -1, 1 if again else 3,
                         1 + roll(60), roll(60), 2500, 2000 + roll(15000), 0 if n % 50 == 0 else 1))

    def chunks(rows, n=50_000):
        for k in range(0, len(rows), n):
            yield rows[k:k + n]

    col.db.execute("begin")
    for c in chunks(note_rows):
        col.db.executemany("insert into notes values (?,?,?,?,?,?,?,?,?,?,?)", c)
    for c in chunks(card_rows):
        col.db.executemany("insert into cards values (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", c)
    for c in chunks(rev_rows):
        col.db.executemany("insert into revlog values (?,?,?,?,?,?,?,?,?)", c)
    col.db.execute("commit")
    # Register the tags, validate rows (what Anki's Tools > Check Database does).
    out, ok = col.fix_integrity()
    print("check database:", ok, out.replace("\n", " | ")[:300], file=sys.stderr)
    col.close()


def measure(path: str, work: str, runs: int) -> dict:
    res: dict = {}

    # Open: Collection() (+ the deck list's counts, what the main window draws first).
    opens = []
    for _ in range(runs):
        t0 = time.perf_counter()
        col = Collection(path)
        col.sched.deck_due_tree()
        opens.append(ms(t0))
        col.close()
    res["open_plus_tree"] = median(opens)
    col = Collection(path)
    res["deck_tree"] = median([timed(col.sched.deck_due_tree) for _ in range(runs)])

    # Build the day's queue (first get_queued_cards after the deck changed), per top deck:
    # Anki studies one deck tree at a time; our study.start is over the whole collection.
    dids = {n: col.decks.id_for_name(n) for n in ["İngilizce", "YKS"]}
    for name, did in dids.items():
        other = dids["YKS" if name == "İngilizce" else "İngilizce"]
        xs = []
        for _ in range(runs):
            col.decks.select(other)
            col.sched.get_queued_cards()
            col.decks.select(did)
            t0 = time.perf_counter()
            q = col.sched.get_queued_cards(fetch_limit=1)
            xs.append(ms(t0))
        res[f"queue_build[{name}]"] = median(xs)
        res[f"queue_counts[{name}]"] = [q.new_count, q.learning_count, q.review_count]

    # Answer "good" + fetch the next card (+ render it, as the reviewer does): our
    # study.answer round trip. 40 answers per run, like the suite.
    col.decks.select(dids["İngilizce"])
    laps, laps_render = [], []
    for _ in range(max(1, runs // 2)):
        q = col.sched.get_queued_cards(fetch_limit=1)
        for _ in range(40):
            qc = q.cards[0]
            card = Card(col, backend_card=qc.card)
            card.start_timer()
            ans = col.sched.build_answer(card=card, states=qc.states, rating=CardAnswer.GOOD)
            t0 = time.perf_counter()
            col.sched.answer_card(ans)
            q = col.sched.get_queued_cards(fetch_limit=1)
            t1 = time.perf_counter()
            nxt = Card(col, backend_card=q.cards[0].card)
            nxt.question()
            nxt.answer()
            laps.append((t1 - t0) * 1000)
            laps_render.append(ms(t0))
    res["answer_next_p50_p95"] = [median(laps), p95(laps)]
    res["answer_next_render_p50_p95"] = [median(laps_render), p95(laps_render)]

    # Custom session by classification: a filtered deck of 200 cards from the same query.
    xs = []
    for k in range(runs):
        fd = col.sched.get_or_create_filtered_deck(deck_id=0)
        fd.name = f"bench {k}"
        fd.config.search_terms[0].search = f"{B1_UP} deck:İngilizce"
        fd.config.search_terms[0].limit = 200
        t0 = time.perf_counter()
        did = col.sched.add_or_update_filtered_deck(fd).id
        q = col.sched.get_queued_cards(fetch_limit=1)  # select happens in add_or_update
        xs.append(ms(t0))
        assert q.cards, "filtered deck is empty"
        col.decks.remove([did])
    res["filtered_deck_200"] = median(xs)

    # Searches: matching ids in browser order (the count) + the first 100 rows.
    col.load_browser_card_columns()
    sort_col = col.get_browser_column("noteFld")
    worst, per = 0.0, {}
    for ours, q in SEARCHES:
        xs = []
        for _ in range(runs):
            t0 = time.perf_counter()
            ids = col.find_cards(q, order=sort_col)
            for cid in ids[:100]:
                list(col.browser_row_for_id(cid)[0])
            xs.append(ms(t0))
        per[ours] = [median(xs), len(ids)]
        worst = max(worst, per[ours][0])
    res["search"] = per
    res["search_worst"] = worst

    # Browser page: whole list sorted, 100 rows at four positions, three orders.
    worst_page, worst_rows = 0.0, 0.0
    for _, key in SORTS:
        column = col.get_browser_column(key)
        for pos in ["0", "1/3", "1/2", "end"]:
            xs, ys = [], []
            for _ in range(runs):
                t0 = time.perf_counter()
                ids = col.find_cards("", order=column)
                off = {"0": 0, "1/3": len(ids) // 3, "1/2": len(ids) // 2, "end": len(ids) - 100}[pos]
                t1 = time.perf_counter()
                for cid in ids[off:off + 100]:
                    list(col.browser_row_for_id(cid)[0])
                xs.append(ms(t0))
                ys.append(ms(t1))
            worst_page = max(worst_page, median(xs))
            worst_rows = max(worst_rows, median(ys))
    res["browser_page_worst"] = worst_page
    res["browser_rows_only_worst"] = worst_rows

    # Statistics: the stats screen's backend call (computed from the revlog every time).
    res["stats_graphs_all_365"] = median([timed(lambda: col._backend.graphs(search="", days=365)) for _ in range(runs)])
    res["stats_graphs_deck_365"] = median(
        [timed(lambda: col._backend.graphs(search="deck:İngilizce", days=365)) for _ in range(runs)])
    some = col.find_cards("is:review")[:runs]
    res["card_info"] = median([timed(lambda c=c: col.card_stats_data(c)) for c in some])

    # Backup (collection only, no media; Anki's own periodic backup).
    bdir = os.path.join(work, "backups")
    os.makedirs(bdir, exist_ok=True)
    xs = []
    for k in range(runs):
        col.set_config("bench_touch", k)  # force=True still skips an unchanged collection
        t0 = time.perf_counter()
        assert col.create_backup(backup_folder=bdir, force=True, wait_for_completion=True)
        xs.append(ms(t0))
    res["backup_collection"] = median(xs)
    res["backup_bytes"] = max(os.path.getsize(os.path.join(bdir, f)) for f in os.listdir(bdir))

    # .apkg of one deck (~8k notes / 17k cards, with scheduling) and its import into an empty collection.
    did = col.decks.id_for_name("İngilizce::Kelimeler 1")
    apkg = os.path.join(work, "deck.apkg")
    opts = ExportAnkiPackageOptions(with_scheduling=True, with_deck_configs=True, with_media=False, legacy=False)
    xs = []
    for _ in range(runs):
        t0 = time.perf_counter()
        exported = col.export_anki_package(out_path=apkg, options=opts, limit=DeckIdLimit(did))
        xs.append(ms(t0))
    res["export_apkg_deck"] = median(xs)
    res["export_apkg_notes"] = exported
    xs = []
    for k in range(runs):
        fresh = os.path.join(work, f"fresh{k}.anki2")
        for f in (fresh, fresh + "-wal"):
            if os.path.exists(f):
                os.remove(f)
        c2 = Collection(fresh)
        req = ImportAnkiPackageRequest(package_path=apkg, options=ImportAnkiPackageOptions(with_scheduling=True))
        t0 = time.perf_counter()
        log = c2.import_anki_package(req)
        xs.append(ms(t0))
        res["import_apkg_notes"] = len(log.log.new)
        c2.close()
    res["import_apkg_deck"] = median(xs)

    # Text import: the suite's 10 000 pasted lines (tab separated) into a new deck.
    target = col.decks.id("İçe aktarılan")
    xs = []
    for k in range(runs):
        tsv = os.path.join(work, f"import{k}.txt")
        with open(tsv, "w", encoding="utf-8") as f:
            for i in range(10_000):
                f.write(f"yeni kelime {k}-{i}\tnew word {k}-{i}\n")
        t0 = time.perf_counter()
        meta = col._backend.get_csv_metadata(CsvMetadataRequest(path=tsv, delimiter=CsvMetadata.TAB))
        meta.deck_id = target
        out = col._backend.import_csv(path=tsv, metadata=meta)
        xs.append(ms(t0))
        assert len(out.log.new) == 10_000, len(out.log.new)
    res["import_text_10k"] = median(xs)

    col.close()
    # Whole-collection package (.colpkg, no media): what "export collection" writes.
    col = Collection(path)
    t0 = time.perf_counter()
    col.export_collection_package(os.path.join(work, "all.colpkg"), include_media=False, legacy=False)
    res["export_colpkg"] = round(ms(t0), 1)
    res["peak_rss_mb"] = round(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024)
    return res


def timed(f) -> float:
    t0 = time.perf_counter()
    f()
    return ms(t0)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", required=True)
    ap.add_argument("--notes", type=int, default=100_000)
    ap.add_argument("--reviews", type=int, default=2_000_000)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--reuse", action="store_true", help="measure the existing build again")
    a = ap.parse_args()
    os.makedirs(a.dir, exist_ok=True)
    pristine = os.path.join(a.dir, "pristine.anki2")
    if not a.reuse or not os.path.exists(pristine):
        if os.path.exists(pristine):
            os.remove(pristine)
        t0 = time.perf_counter()
        build(pristine, a.notes, a.reviews)
        print(f"build {ms(t0) / 1000:.0f} s", file=sys.stderr)
    # Every measurement starts from the same copy (answers, imports and backups write).
    work = os.path.join(a.dir, "work")
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work)
    path = os.path.join(work, "collection.anki2")
    shutil.copy(pristine, path)
    res = measure(path, work, a.runs)
    res["shape"] = {"notes": a.notes, "reviews": a.reviews, "runs": a.runs}
    print(json.dumps(res, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
