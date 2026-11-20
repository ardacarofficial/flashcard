import { describe, expect, it } from 'vitest';
import { MAX_SENTENCE, capturePage, sentenceAround, squash, toCapture } from '../../src/lib/context';

const block = 'Badgers live in setts. The badger dug a deep hole under the old oak. Then it slept.';
const at = (word: string): [number, number] => {
  const i = block.indexOf(word);
  return [i, i + word.length];
};

describe('sentenceAround', () => {
  it('finds the sentence containing the selection', () => {
    expect(sentenceAround(block, ...at('dug'))).toBe('The badger dug a deep hole under the old oak.');
    expect(sentenceAround(block, ...at('Badgers'))).toBe('Badgers live in setts.');
    expect(sentenceAround(block, ...at('slept'))).toBe('Then it slept.');
  });

  it('keeps abbreviations-like dots without a following space and stops at line breaks', () => {
    expect(sentenceAround('Version 1.5 is out now! Get it', 0, 7)).toBe('Version 1.5 is out now!');
    expect(sentenceAround('Title line\nsecond line here', 11, 17)).toBe('second line here');
  });

  it('handles Turkish and CJK punctuation and multi-char endings', () => {
    const tr = 'Kedi uyudu. Köpek bahçede koştu mu?! Evet.';
    const s = tr.indexOf('bahçede');
    expect(sentenceAround(tr, s, s + 7)).toBe('Köpek bahçede koştu mu?!');
    const ja = '猫が寝た。 犬が走った。';
    expect(sentenceAround(ja, ja.indexOf('犬'), ja.indexOf('犬') + 1)).toBe('犬が走った。');
  });

  it('cuts very long sentences around the selection', () => {
    const long = `${'a '.repeat(600)}TARGET ${'b '.repeat(600)}.`;
    const i = long.indexOf('TARGET');
    const s = sentenceAround(long, i, i + 6);
    expect(s).toContain('TARGET');
    expect(s.length).toBeLessThanOrEqual(MAX_SENTENCE + 2);
    expect(s.startsWith('…')).toBe(true);
    expect(s.endsWith('…')).toBe(true);
  });

  it('returns empty for unknown offsets', () => {
    expect(sentenceAround('', 0, 0)).toBe('');
    expect(sentenceAround(block, -1, -1)).toBe('');
    expect(sentenceAround(block, 5, 500)).toBe('');
  });
});

describe('toCapture', () => {
  it('squashes whitespace and drops a sentence equal to the selection', () => {
    const c = toCapture({ text: '  deep\n hole ', block, start: block.indexOf('deep'), end: block.indexOf('deep') + 9, title: ' Badgers ', url: 'https://e.x/b' });
    expect(c).toEqual({ front: 'deep hole', sentence: 'The badger dug a deep hole under the old oak.', title: 'Badgers', url: 'https://e.x/b' });
    expect(toCapture({ text: 'Then it slept.', block, ...{ start: block.indexOf('Then'), end: block.length }, title: '', url: '' }).sentence).toBe('');
    expect(squash('a  b')).toBe('a b');
  });
});

describe('capturePage (in-page script)', () => {
  it('reports the selection with its block and offsets', () => {
    document.title = 'Badgers';
    document.body.innerHTML = `<article><p id="p">${block}</p></article>`;
    const text = document.getElementById('p')!.firstChild!;
    const range = document.createRange();
    const [s, e] = at('badger dug');
    range.setStart(text, s);
    range.setEnd(text, e);
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    sel.addRange(range);
    const got = capturePage();
    expect(got.text).toBe('badger dug');
    expect(got.title).toBe('Badgers');
    expect(got.block.slice(got.start, got.end)).toBe('badger dug');
    expect(toCapture(got).sentence).toBe('The badger dug a deep hole under the old oak.');
  });

  it('returns no block when nothing is selected', () => {
    window.getSelection()!.removeAllRanges();
    expect(capturePage()).toMatchObject({ text: '', block: '', start: -1 });
  });
});
