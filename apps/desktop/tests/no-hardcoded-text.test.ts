/**
 * Rule 8 (CLAUDE.md), v1.0 "tüm arayüz metinleri çeviri anahtarlarından gelir (lint ile)".
 *
 * An AST lint over every component (`svelte/compiler` `parse`, modern AST), plus checks on
 * the TypeScript next to them:
 *
 * 1. Markup: no text node with letters (keycaps in `<kbd>` are notation); no literal user-visible attribute (`placeholder`,
 *    `title`, `alt`, `label`, `aria-*` texts, button `value`; text-like props of components);
 *    no expression that *renders* a string literal with letters (`{x ? 'On' : 'Off'}`,
 *    `title={'Hi'}`, `` {`${n} cards`} ``). Arguments of calls (`t('key')`, `td(key)`) are
 *    not rendered, so keys are fine.
 * 2. Keys: every literal key handed to `t()` / `td()` / `label_key:` / `messageKey:` exists in
 *    the merged EN **and** TR dictionaries (`t` is typed; `td` takes runtime keys, so a
 *    literal typo there would only show up as a raw key on screen).
 * 3. Scripts (`.ts` and components' `<script>`): no string literal that reads like an English sentence (two or more
 *    words) — UI state set from a `.ts` helper must also be a key.
 *
 * Exceptions, each with its reason: keycaps (`<kbd>`), tests, and the development-only mock
 * backend (`mock.ts`, `api/mock/`).
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { parse } from 'svelte/compiler';
import { describe, expect, it } from 'vitest';
import { flatKeys } from '../src/lib/i18n/core';
import { mergeLocales } from '../scripts/merge-locales.mjs';

const SRC = join(__dirname, '..', 'src');

function files(dir: string, ext: RegExp): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return name === 'generated' ? [] : files(p, ext);
    return ext.test(name) ? [p] : [];
  });
}

const LETTER = /\p{L}/u;

/** Attributes of HTML elements whose value is shown to (or read out for) the user. */
const VISIBLE_ATTRS = new Set([
  'placeholder',
  'title',
  'alt',
  'label',
  'aria-label',
  'aria-description',
  'aria-placeholder',
  'aria-roledescription',
  'aria-valuetext',
]);
/** Component props that carry display text (`<ConfirmDialog message=…>`, `label=…`). */
const VISIBLE_PROP = /(label|title|text|message|placeholder|heading|caption|hint|description)$/i;

// ------------------------------------------------------------------ AST helpers

/** Loosely typed ESTree / Svelte AST node. */
interface Node {
  type: string;
  [k: string]: unknown;
}
const isNode = (v: unknown): v is Node => typeof v === 'object' && v !== null && 'type' in v;

/** String literals an expression can evaluate to (and so render). Calls are opaque. */
export function renderedStrings(expr: unknown): string[] {
  if (!isNode(expr)) return [];
  switch (expr.type) {
    case 'Literal':
      return typeof expr.value === 'string' ? [expr.value] : [];
    case 'TemplateLiteral':
      return [(expr.quasis as Node[]).map((q) => (q.value as { cooked: string }).cooked).join(' ')];
    case 'ConditionalExpression':
      return [...renderedStrings(expr.consequent), ...renderedStrings(expr.alternate)];
    case 'LogicalExpression':
      return [...(expr.operator === '&&' ? [] : renderedStrings(expr.left)), ...renderedStrings(expr.right)];
    case 'BinaryExpression':
      return expr.operator === '+' ? [...renderedStrings(expr.left), ...renderedStrings(expr.right)] : [];
    case 'SequenceExpression':
      return renderedStrings((expr.expressions as Node[]).at(-1));
    default:
      return [];
  }
}

const isText = (s: string) => LETTER.test(s);

function attrValue(attr: Node): string[] {
  const value = attr.value;
  if (value === true || value == null) return [];
  const parts = (Array.isArray(value) ? value : [value]) as Node[];
  return parts.flatMap((p) =>
    p.type === 'Text' ? [String(p.data)] : p.type === 'ExpressionTag' ? renderedStrings(p.expression) : [],
  );
}

function isButtonInput(el: Node): boolean {
  if (el.name !== 'input') return false;
  const type = (el.attributes as Node[]).find((a) => a.type === 'Attribute' && a.name === 'type');
  const v = type ? attrValue(type).join('') : '';
  return ['button', 'submit', 'reset'].includes(v);
}

/** Every hard-coded user-visible text in a component's markup. */
export function findRawText(source: string): string[] {
  const ast = parse(source, { modern: true });
  const hits: string[] = [];
  const visit = (node: unknown): void => {
    if (Array.isArray(node)) return node.forEach(visit);
    if (!isNode(node)) return;
    switch (node.type) {
      case 'Text':
        if (isText(String(node.data))) hits.push(String(node.data).trim());
        return;
      case 'ExpressionTag':
        for (const s of renderedStrings(node.expression)) if (isText(s)) hits.push(`{${s.trim()}}`);
        return;
      case 'Attribute': {
        // `node` is the attribute; its element was handled by the caller (see below).
        return;
      }
      case 'RegularElement':
      case 'SvelteElement':
      case 'Component':
      case 'SvelteComponent': {
        // Keycaps (`<kbd>Esc</kbd>`) are notation printed on the keyboard, the same in TR/EN.
        if (node.name === 'kbd') return;
        const component = node.type === 'Component' || node.type === 'SvelteComponent';
        for (const attr of node.attributes as Node[]) {
          if (attr.type !== 'Attribute') continue;
          const name = String(attr.name);
          const shown = component
            ? VISIBLE_PROP.test(name)
            : VISIBLE_ATTRS.has(name) || (name === 'value' && isButtonInput(node));
          if (!shown) continue;
          for (const s of attrValue(attr)) if (isText(s)) hits.push(`${name}="${s.trim()}"`);
        }
        visit(node.fragment);
        return;
      }
      default:
        // Blocks (if/each/await/key/snippet), fragments, special elements: walk children,
        // never into expressions (conditions, each sources) — those are not rendered.
        for (const [k, v] of Object.entries(node)) {
          if (k === 'expression' || k === 'test' || k === 'attributes') continue;
          if (Array.isArray(v) || isNode(v)) visit(v);
        }
    }
  };
  visit(ast.fragment);
  return hits;
}

// ------------------------------------------------------------------ keys

const merged = mergeLocales() as Record<string, unknown>;
const KEYS = {
  en: new Set(flatKeys(merged.en)),
  tr: new Set(flatKeys(merged.tr)),
};

/** Literal i18n keys a source file hands to the translation layer. */
export function literalKeys(source: string): string[] {
  const out: string[] = [];
  const res = [
    /\b(?:t|td)\(\s*(['"])([\w.-]+)\1/g,
    /\b(?:label_key|messageKey|message_key|name_key)\s*:\s*(['"])([\w.-]+)\1/g,
  ];
  for (const re of res) for (const m of source.matchAll(re)) out.push(m[2]!);
  return out;
}

// ------------------------------------------------------------------ scripts

/**
 * Two or more words of letters, starting with a capital: reads like a sentence for a person.
 * Keys (`a.b_c`), CSS, method names, query syntax and identifiers never match.
 */
const SENTENCE = /^[\p{Lu}][\p{L}'’]*(?:[ ,][\p{L}'’,]+)+[.!?…:]?$/u;


export function sentenceLiterals(source: string): string[] {
  const code = source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/.*$/gm, '$1');
  const out: string[] = [];
  for (const m of code.matchAll(/(['"`])((?:(?!\1)[^\\\n]|\\.)*)\1/g)) {
    const s = m[2]!;
    if (!s.includes('${') && SENTENCE.test(s.trim())) out.push(s);
  }
  return out;
}

// ------------------------------------------------------------------ tests

describe('no hard-coded UI text', () => {
  it('detector catches literal text, attributes, props and rendered literals', () => {
    expect(
      findRawText(
        `<script>let x = $state(1); const s = 'Hidden';</script>
         <p class="row" title="Hi">Hello {x ? 'On' : t('k.on')}</p>
         <b>{t("x", { n: 1 })}</b><i aria-label={'Close'}></i>
         {#if x}<Dialog message="Sure?" kind="danger" />{:else}{\`\${x} cards\`}{/if}
         <input type="submit" value="Send" /><input value="raw" />
         {#each [1] as i (i)}<span>{i} × {x}</span>{/each}<kbd>{'Esc'}</kbd>
         <style>p { color: red; }</style>`,
      ),
    ).toEqual(['title="Hi"', 'Hello', '{On}', 'aria-label="Close"', 'message="Sure?"', '{cards}', 'value="Send"']);
  });

  it('key extraction and sentence heuristic', () => {
    expect(literalKeys(`t('deck.new'); td("errors.x", p); { label_key: 'nav.a' }; td(key)`)).toEqual([
      'deck.new',
      'errors.x',
      'nav.a',
    ]);
    expect(
      sentenceLiterals(`const a = 'Something went wrong'; const b = 'deck.new'; // 'Not this one'
        const c = 'fc-row is-open'; const d = "Delete this?"; const e = \`\${n} items\`;`),
    ).toEqual(['Something went wrong', 'Delete this?']);
  });

  for (const file of files(SRC, /\.svelte$/)) {
    it(`markup: ${relative(SRC, file)}`, () => {
      expect(findRawText(readFileSync(file, 'utf8'))).toEqual([]);
    });
  }

  it('every literal key exists in EN and TR', () => {
    const missing: string[] = [];
    for (const file of files(SRC, /\.(svelte|ts)$/)) {
      // Development-only mocks hand fake plugin keys around (`plugin.<id>.*` at runtime).
      if (file.endsWith('.test.ts') || file.endsWith('mock.ts')) continue;
      for (const key of literalKeys(readFileSync(file, 'utf8'))) {
        for (const lang of ['en', 'tr'] as const) {
          if (!KEYS[lang].has(key)) missing.push(`${relative(SRC, file)}: ${lang} ${key}`);
        }
      }
    }
    expect(missing).toEqual([]);
  });

  it('scripts have no sentence-like literals', () => {
    const found: string[] = [];
    for (const file of files(SRC, /\.(svelte|ts)$/)) {
      const rel = relative(SRC, file);
      // Mocks (`mock.ts`, `api/mock/`) are development-only fake data, never shipped UI text.
      if (rel.endsWith('.test.ts') || /mock(\.ts|\/)/.test(rel)) continue;
      const text = readFileSync(file, 'utf8');
      const code = file.endsWith('.svelte')
        ? [...text.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]).join('\n')
        : text;
      for (const s of sentenceLiterals(code)) found.push(`${rel}: ${s}`);
    }
    expect(found).toEqual([]);
  });
});
