import { describe, expect, it } from 'vitest';
import {
  completeLink,
  firstMediaHash,
  insertAt,
  linkQueryAt,
  mapMediaSrc,
  maskFromDrag,
  mediaHashes,
  mediaSnippet,
  move,
  parseMasks,
  sampleFields,
  scopeCss,
  cssIsContained,
  serializeMasks,
  wrapSelection,
} from './content';

const H = '9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08';

describe('card content helpers', () => {
  it('maps media references to URLs', () => {
    const html = `<img src="media:${H}" alt=""><audio controls src="media:${H}"></audio><img src="x">`;
    expect(mediaHashes(html)).toEqual([H]);
    expect(mapMediaSrc(html, (h) => `fc://${h.slice(0, 4)}`)).toBe(
      '<img src="fc://9f86" alt=""><audio controls src="fc://9f86"></audio><img src="x">',
    );
    expect(firstMediaHash(`junk media:${H}`)).toBe(H);
    expect(firstMediaHash('none')).toBeNull();
  });

  it('edits text around the selection', () => {
    expect(wrapSelection('a dog', 2, 5, '**', '**')).toEqual({ text: 'a **dog**', start: 4, end: 7 });
    expect(insertAt('ab', 1, 1, 'X')).toEqual({ text: 'aXb', start: 2, end: 2 });
  });

  it('finds and completes [[links]]', () => {
    expect(linkQueryAt('see [[Por', 9)).toBe('Por');
    expect(linkQueryAt('see [[Por]] x', 13)).toBeNull();
    expect(linkQueryAt('no link', 7)).toBeNull();
    expect(completeLink('see [[Por', 9, 'Porsuk')).toEqual({ text: 'see [[Porsuk]]', start: 14, end: 14 });
    expect(completeLink('[[Po]] rest', 4, 'Porsuk').text).toBe('[[Porsuk]] rest');
  });

  it('builds media snippets by kind', () => {
    const base = { reference: `media:${H}`, original_name: 'a [b].png' };
    expect(mediaSnippet({ ...base, kind: 'image' })).toBe(`![a b.png](media:${H})`);
    expect(mediaSnippet({ ...base, kind: 'audio' })).toBe(`<audio controls src="media:${H}"></audio>`);
  });

  it('scopes CSS and moves items', () => {
    expect(scopeCss('.x { color: red }', 'fc-card-1')).toBe('.card.fc-card-1 {\n.x { color: red }\n}');
    expect(scopeCss('  ', 'fc-card-1')).toBe('');
    expect(scopeCss('.x::after { content: "}" } /* } */', 's')).toContain('content: "}"');
    expect(scopeCss('.x { content: "\\"}" }', 's')).not.toBe('');
    for (const evil of ['} body { color: red }', '.a {', '.a { b: rgb( }', '</style><b>', '.a { content: "x }', '/* }']) {
      expect(scopeCss(evil, 's')).toBe('');
    }
    expect(move([1, 2, 3], 0, 1)).toEqual([2, 1, 3]);
    expect(move([1, 2, 3], 0, -1)).toEqual([1, 2, 3]);
  });

  it('samples fields for note type previews', () => {
    const f = (key: string, name: string, kind: 'rich_text' | 'image' = 'rich_text') => ({
      key, name, kind, required: false, unique: false, searchable: true, default_value: null,
    });
    const def = { name: 'x', kind: 'cloze' as const, fields: [f('text', 'Text'), f('img', 'Img', 'image'), f('', 'New')], templates: [], css: '', sort_field: 'text' };
    expect(sampleFields(def)).toEqual({ text: '{{c1::Text}} …' });
  });

  it('draws masks with stable numbers', () => {
    const m = parseMasks('{"mode":"hide_one","masks":[{"n":3,"x":0,"y":0,"w":0.1,"h":0.1}]}');
    expect(m.mode).toBe('hide_one');
    const next = maskFromDrag(m.masks, 0.5, 0.6, 0.2, 0.1)!;
    expect(next).toEqual({ n: 4, x: 0.2, y: 0.1, w: 0.3, h: 0.5 });
    expect(maskFromDrag([], 0.1, 0.1, 0.105, 0.5)).toBeNull();
    expect(maskFromDrag([], -1, -1, 2, 2)).toMatchObject({ x: 0, y: 0, w: 1, h: 1 });
    expect(parseMasks('{nope')).toEqual({ mode: 'hide_all', masks: [] });
    expect(serializeMasks({ mode: 'hide_all', masks: [] })).toBe('');
    expect(JSON.parse(serializeMasks({ mode: 'hide_all', masks: [next] }))).toEqual({ mode: 'hide_all', masks: [next] });
  });
});
