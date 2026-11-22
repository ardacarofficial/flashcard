// Release acceptance with the REAL Subtitle client (v1.4, docs/06 §5.1): the extension's own
// outbox (`outbox.ts`, its queue, backoff and retry) and HTTP client (`api.ts`), unchanged,
// against a real `fc api serve`, with the exchange broken on purpose by `harness.Proxy`:
//
// 1. media uploaded, then the line drops while the server works on the capture → retry →
//    exactly one card, each picture/sound stored once;
// 2. the server committed but its answer is lost (socket reset) → retry → `duplicate`, same card;
// 3. the server process is SIGKILLed mid-batch and restarted → the outbox drains → no
//    duplicates, nothing lost (several rounds, the kill moved until it lands mid-batch);
// 4. undo of a capture in the app, then the same capture id again (the extension's pending
//    retry) → a NEW card (documented: undo forgets the capture; docs/06 §5.1);
// 5. backup (`io.backup.create`), restore into a fresh profile: the same notes, cards and
//    media, an audio file byte-identical to what the extension sent; the restored profile
//    still knows every capture id (a resend is a duplicate) and has revoked the keys;
// 6. `subtitle.captures.recent` tells the truth about every attempt above.
//
// Run by `check.mjs` (after the plain contract); exported so it shares that process.
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { FcServer, MemoryStorage, Proxy, fcCall, jpegOf, loadExtension, makeCapture, wavOf } from './harness.mjs';

const CAPTURE = (m, p) => m === 'POST' && p === '/subtitle/v1/captures';
const BATCH = (m, p) => m === 'POST' && p === '/subtitle/v1/captures:batch';
const MEDIA = (m, p) => m === 'POST' && p === '/subtitle/v1/media';
const HOURS = 3600_000;

export async function runInterrupted() {
  const ext = await loadExtension();
  const dir = mkdtempSync(join(tmpdir(), 'fc-subtitle-interrupted-'));
  const db = join(dir, 'collection.db');
  const call = fcCall(db);
  let server = new FcServer(db);
  let restored = null;
  const proxy = new Proxy();
  try {
    call('modules.set_enabled', { id: 'subtitle', enabled: true });
    const { secret } = call('subtitle.keys.create', { name: 'interruption check' });
    await server.start();
    await proxy.listen();
    proxy.upstream = server.port;

    // The extension's outbox, its server set to the proxy; a clock the check moves past
    // any backoff ("later").
    let clock = Date.now();
    const later = () => {
      clock += 7 * HOURS;
    };
    const storage = new MemoryStorage({ [ext.KEYS.server]: JSON.stringify({ url: proxy.url(), token: secret, deckId: null }) });
    const outbox = new ext.Outbox({
      storage,
      now: () => clock,
      random: () => 0.5,
      online: () => true,
      api: (s) => new ext.FlashcardsApi(s.url, s.token, undefined, 15_000),
    });
    const drained = async () => {
      const st = await outbox.status();
      assert.equal(st.pending, 0, `outbox still pending: ${JSON.stringify(st)}`);
      assert.equal(st.failed, 0, `outbox failed: ${JSON.stringify(st.failures)}`);
    };

    // What the app has, read back from the persisted collection.
    const subtitleCards = () => call('search.cards', { query: 'deck:Subtitle', limit: 500 });
    const mediaCount = () => call('media.list').items.length;
    /** Attempts of one capture, oldest first. */
    const attempts = (id) =>
      call('subtitle.captures.recent', { limit: 500 })
        .attempts.filter((a) => a.capture_id === id)
        .reverse();
    const captureExchanges = (from) => proxy.exchanges.slice(from).filter((e) => CAPTURE(e.method, e.path));
    const noteOfCard = (cardId) => {
      const item = subtitleCards().items.find((c) => String(c.card_id) === String(cardId));
      assert.ok(item, `card ${cardId} is in deck:Subtitle`);
      return call('notes.get', { id: item.note_id });
    };

    // ---------------------------------------------------------------- 1. drop mid-capture
    {
      const before = { cards: subtitleCards().total, media: mediaCount() };
      const from = proxy.exchanges.length;
      const c = makeCapture(1, 'lantern', 'He lifted the lantern slowly.');
      const dropped = proxy.once(CAPTURE, 'drop');
      const first = await outbox.add(c, { screenshot: { data: jpegOf(1), mime: 'image/jpeg' }, audio: { data: wavOf(1), mime: 'audio/wav' } });
      await dropped;
      assert.equal(first.status, 'queued', `the extension keeps it queued: ${JSON.stringify(first)}`);
      later();
      await outbox.flush();
      await drained();
      assert.equal(subtitleCards().total, before.cards + 1, 'exactly one card');
      assert.equal(mediaCount(), before.media + 2, 'the picture and the sound stored once each');
      const uploads = proxy.exchanges.slice(from).filter((e) => MEDIA(e.method, e.path));
      assert.equal(uploads.length, 4, 'both media uploaded twice (first try, retry)');
      assert.equal(new Set(uploads.map((u) => u.body.upload_id)).size, 2, 'a re-upload keeps its upload id');
      const rows = attempts(c.capture_id);
      const created = rows.filter((r) => r.result === 'created');
      assert.equal(created.length, 1, `one created attempt: ${JSON.stringify(rows)}`);
      assert.ok(rows.every((r) => String(r.card_id) === String(created[0].card_id)), 'every attempt names the same card');
      // The dropped request either reached the server (created, then the retry is a duplicate)
      // or not (the retry creates): the log says which, never more.
      assert.deepEqual(rows.map((r) => r.result), rows.length === 2 ? ['created', 'duplicate'] : ['created']);
      const note = noteOfCard(created[0].card_id);
      assert.ok(note.fields.image.startsWith('media:') && note.fields.audio.startsWith('media:'), 'media attached');
      console.log(`ok  1. drop mid-capture → one card, media once (server ${rows.length === 2 ? 'had' : 'had not'} committed before the drop)`);
    }

    // ---------------------------------------------------------------- 2. answer lost after commit
    let audioCapture;
    {
      const before = subtitleCards().total;
      const from = proxy.exchanges.length;
      const c = makeCapture(2, 'harbour', 'The ship left the harbour at dawn.');
      const lost = proxy.once(CAPTURE, 'lose');
      const first = await outbox.add(c, { audio: { data: wavOf(2), mime: 'audio/wav' } });
      const ex = await lost;
      assert.equal(first.status, 'queued');
      assert.equal(ex.status, 201);
      assert.equal(ex.body.status, 'created');
      assert.equal(ex.delivered, false);
      later();
      await outbox.flush();
      await drained();
      const [, retry] = captureExchanges(from);
      assert.equal(retry.status, 200);
      assert.deepEqual(retry.body, { status: 'duplicate', card_id: ex.body.card_id }, 'the retry is a duplicate of the same card');
      assert.equal(subtitleCards().total, before + 1);
      const rows = attempts(c.capture_id);
      assert.deepEqual(rows.map((r) => [r.result, String(r.card_id)]), [['created', ex.body.card_id], ['duplicate', ex.body.card_id]]);
      audioCapture = { capture: c, cardId: ex.body.card_id, wav: wavOf(2) };
      console.log('ok  2. answer lost after commit → retry is duplicate, same card');
    }

    // ---------------------------------------------------------------- 4. undo, then the same id
    {
      const before = subtitleCards().total;
      const from = proxy.exchanges.length;
      const c = makeCapture(4, 'meadow', 'Cows grazed in the meadow.');
      const lost = proxy.once(CAPTURE, 'lose');
      await outbox.add(c, { screenshot: { data: jpegOf(4), mime: 'image/jpeg' } });
      const ex = await lost;
      assert.equal(ex.body.status, 'created');
      assert.equal(subtitleCards().total, before + 1);
      // The user undoes it in the app while the extension's retry still waits.
      const undone = call('oplog.undo');
      assert.ok(undone, 'something was undone');
      assert.equal(subtitleCards().total, before, 'undo removed the card');
      later();
      await outbox.flush();
      await drained();
      const [, retry] = captureExchanges(from);
      assert.equal(retry.status, 201, 'documented: after undo the same capture id is a new capture');
      assert.equal(retry.body.status, 'created');
      assert.notEqual(retry.body.card_id, ex.body.card_id, 'a new card, not the undone one');
      assert.equal(subtitleCards().total, before + 1, 'one card again, not two');
      assert.ok(noteOfCard(retry.body.card_id).fields.image.startsWith('media:'), 'the re-created card has its picture');
      const rows = attempts(c.capture_id);
      assert.deepEqual(rows.map((r) => [r.result, String(r.card_id)]), [['created', ex.body.card_id], ['created', retry.body.card_id]]);
      console.log('ok  4. undo, then the same capture id → a new card (documented), log shows both');
    }

    // ---------------------------------------------------------------- 3. server killed mid-batch
    {
      let delayMs = 40;
      let lo = 0;
      let hi = 400;
      let midHit = 0;
      let n = 100;
      for (let round = 0; round < 8 && !(midHit > 0 && round >= 3); round++) {
        const before = { cards: subtitleCards().total, media: mediaCount() };
        const from = proxy.exchanges.length;
        const batch = [];
        for (let i = 0; i < 20; i++, n++) {
          const c = makeCapture(n, `word${n}`, `Sentence number ${n} has word${n} in it.`);
          const media = i % 2 === 0 ? { screenshot: { data: jpegOf(n), mime: 'image/jpeg' }, audio: { data: wavOf(n), mime: 'audio/wav' } } : {};
          assert.equal((await outbox.enqueue(c, media)).status, 'queued');
          batch.push({ c, media: i % 2 === 0 });
        }
        const killed = proxy.once(BATCH, { kill: () => server.kill(), delayMs });
        await outbox.flush();
        await killed;
        assert.equal((await outbox.status()).pending, 20, 'the whole batch waits for a retry');
        server = await new FcServer(db).start();
        proxy.upstream = server.port;
        later();
        await outbox.flush();
        await drained();
        const after = subtitleCards().total;
        assert.equal(after, before.cards + 20, `no duplicates, nothing lost (round ${round})`);
        assert.equal(mediaCount(), before.media + 20, 'every picture and sound once');
        const retried = proxy.exchanges.slice(from).filter((e) => BATCH(e.method, e.path)).at(-1);
        assert.equal(retried.status, 200);
        const statuses = retried.body.results.map((r) => r.status);
        assert.ok(statuses.every((s) => s === 'created' || s === 'duplicate'), JSON.stringify(statuses));
        const committed = statuses.filter((s) => s === 'duplicate').length;
        // Each capture: one `created` row (or none, when the kill fell between the capture's
        // commit and its log row: then the first row is the retry's `duplicate`), the same card.
        for (const { c, media } of batch) {
          const rows = attempts(c.capture_id);
          const card = retried.body.results.find((r) => r.capture_id === c.capture_id).card_id;
          assert.ok(rows.length >= 1 && rows.every((r) => String(r.card_id) === String(card)), `${c.capture_id}: ${JSON.stringify(rows)}`);
          const created = rows.filter((r) => r.result === 'created').length;
          assert.ok(created === 1 || (created === 0 && rows[0].result === 'duplicate'), `${c.capture_id}: ${JSON.stringify(rows)}`);
          if (media) {
            const note = noteOfCard(card);
            assert.ok(note.fields.image.startsWith('media:') && note.fields.audio.startsWith('media:'), `${c.capture_id} has its media`);
          }
        }
        console.log(`ok  3. kill ${delayMs} ms into the batch: ${committed}/20 committed before the kill → restart → drained, 20 cards`);
        if (committed > 0 && committed < 20) midHit++;
        // Move the kill toward the middle of the batch.
        if (committed === 0) lo = delayMs;
        else if (committed === 20) hi = delayMs;
        else if (committed < 10) lo = delayMs;
        else hi = delayMs;
        delayMs = Math.max(1, Math.round((lo + hi) / 2));
      }
      if (midHit === 0) console.log('warn 3. no round landed strictly mid-batch (machine timing); the invariants held in every round');
    }

    // ---------------------------------------------------------------- 5. backup → fresh profile
    {
      const backup = join(dir, 'after-interruptions.fcbackup');
      const job = call('io.backup.create', { path: backup });
      assert.equal(job.status, 'succeeded', JSON.stringify(job));
      const freshDir = mkdtempSync(join(tmpdir(), 'fc-subtitle-restored-'));
      const fresh = join(freshDir, 'collection.db');
      const callFresh = fcCall(fresh);
      try {
        const r = callFresh('io.backup.restore', { path: backup, safety_backup_path: join(freshDir, 'pre-restore.fcbackup') });
        assert.equal(r.status, 'succeeded', JSON.stringify(r));
        const all = (c) => c('search.cards', { query: '', limit: 1 }).total;
        const notes = (c) => c('search.cards', { query: '', notes_only: true, limit: 1 }).total;
        const media = (c) => c('media.list').items.length;
        assert.equal(all(callFresh), all(call), 'cards');
        assert.equal(notes(callFresh), notes(call), 'notes');
        assert.equal(callFresh('search.cards', { query: 'deck:Subtitle', limit: 1 }).total, subtitleCards().total, 'Subtitle cards');
        assert.equal(media(callFresh), media(call), 'media');
        const note = noteOfCard(audioCapture.cardId);
        const hash = note.fields.audio.slice('media:'.length);
        const audio = callFresh('media.read', { hash });
        assert.equal(audio.mime, 'audio/wav');
        assert.equal(audio.data_base64, audioCapture.wav, 'the restored sound is byte-identical to what the extension sent');
        // The restored profile: diagnostics are not user data (not in a backup), keys are revoked.
        assert.deepEqual(callFresh('subtitle.captures.recent').attempts, [], 'a restore brings no capture diagnostics');
        restored = new FcServer(fresh);
        await restored.start();
        const url = `http://127.0.0.1:${restored.port}/subtitle`;
        await assert.rejects(new ext.FlashcardsApi(url, secret).me(), (e) => e.kind === 'auth', 'a restore revokes the keys');
        // With a new key the extension's resend of a capture it already sent is a duplicate of
        // the same card: the restore kept which capture ids the app has.
        const { secret: second } = callFresh('subtitle.keys.create', { name: 'after restore' });
        assert.equal(await new ext.FlashcardsApi(url, second).createCapture({ ...audioCapture.capture, media: {} }), 'duplicate');
        const [row] = callFresh('subtitle.captures.recent').attempts;
        assert.deepEqual([row.capture_id, row.result, String(row.card_id)], [audioCapture.capture.capture_id, 'duplicate', String(audioCapture.cardId)]);
        console.log('ok  5. backup → fresh profile: same notes/cards/media, audio byte-identical, capture ids kept, keys revoked');
      } finally {
        await restored?.kill();
        restored = null;
        rmSync(freshDir, { recursive: true, force: true });
      }
    }
    console.log('ok  6. subtitle.captures.recent matched every attempt above');
  } finally {
    await server.kill();
    await restored?.kill();
    await proxy.close();
    rmSync(dir, { recursive: true, force: true });
  }
}
