/**
 * Filter chips of the Search screen (v1.4 E3): each chip WRITES one predicate of the query
 * language into the query text (`durum:yeni`, `sıklık<=1000`), so the user sees — and learns —
 * the language. Only presentation of the query text: Rust still parses and runs the query
 * (fc-core `query::lang`, docs/02 §9.1). The keys below are exactly the parser's keys.
 */
import type { MessageKey } from '$lib/i18n/index.svelte';

/** One value of a chip's menu: the token it writes and its label. */
export interface ChipValue {
  /** The whole token (`durum:yeni`, `karşılaşma>=2`). */
  token: string;
  /** i18n key of its label; or a ready text (`raw`, e.g. a language name from `Intl`). */
  label: MessageKey | { raw: string };
}

export interface ChipDef {
  id: string;
  label: MessageKey;
  /** Folded keys that count as this chip's predicate (Turkish and English aliases). */
  keys: string[];
  /** Module the predicate needs (the chip is hidden while it is off). */
  module?: string;
  /** Menu values; `text` chips instead write `key:` and let the user type the value. */
  values: ChipValue[];
  /** For `text` chips: the key written into the query (`kelime:`). */
  text?: string;
}

const LANGS = ['en', 'de', 'fr', 'es', 'it', 'pt', 'ru', 'ja', 'ko', 'zh'];

function langName(code: string, locale: string): string {
  try {
    return new Intl.DisplayNames([locale], { type: 'language' }).of(code) ?? code;
  } catch {
    return code;
  }
}

/** Every chip, in screen order; `locale` names the languages. */
export function chipDefs(locale = 'tr'): ChipDef[] {
  return [
    {
      id: 'status',
      label: 'browse.chips.status',
      keys: ['durum', 'status'],
      module: 'words',
      values: [
        { token: 'durum:yeni', label: 'browse.chips.status_new' },
        { token: 'durum:öğreniyorum', label: 'browse.chips.status_learning' },
        { token: 'durum:biliyorum', label: 'browse.chips.status_known' },
        { token: 'durum:yoksay', label: 'browse.chips.status_ignored' },
      ],
    },
    {
      id: 'lang',
      label: 'browse.chips.lang',
      keys: ['dil', 'lang', 'language'],
      module: 'words',
      values: [
        ...LANGS.map((l) => ({ token: `dil:${l}`, label: { raw: `${langName(l, locale)} (${l})` } })),
        { token: 'dil:yok', label: 'browse.chips.lang_none' as MessageKey },
      ],
    },
    {
      id: 'cefr',
      label: 'browse.chips.cefr',
      keys: ['cefr', 'level', 'seviye', 'duzey'],
      module: 'word_data',
      values: ['A1', 'A2', 'B1', 'B2', 'C1', 'C2'].map((b) => ({ token: `cefr:${b}`, label: { raw: b } })),
    },
    {
      id: 'freq',
      label: 'browse.chips.freq',
      keys: ['siklik', 'freq', 'frequency'],
      module: 'word_data',
      values: [
        { token: 'sıklık<=1000', label: 'browse.chips.freq_1000' },
        { token: 'sıklık<=2000', label: 'browse.chips.freq_2000' },
        { token: 'sıklık<=5000', label: 'browse.chips.freq_5000' },
        { token: 'sıklık<=10000', label: 'browse.chips.freq_10000' },
        { token: 'sıklık>10000', label: 'browse.chips.freq_rare' },
      ],
    },
    {
      id: 'from',
      label: 'browse.chips.from',
      keys: ['nereden', 'from'],
      values: [
        { token: 'nereden:altyazı', label: 'browse.chips.from_subtitle' },
        { token: 'nereden:okuma', label: 'browse.chips.from_reading' },
        { token: 'nereden:elle', label: 'browse.chips.from_manual' },
        { token: 'nereden:anki', label: 'browse.chips.from_anki' },
        { token: 'nereden:içe_aktarma', label: 'browse.chips.from_import' },
      ],
    },
    {
      id: 'seen',
      label: 'browse.chips.seen',
      keys: ['karsilasma', 'seen'],
      module: 'subtitle',
      values: [
        { token: 'karşılaşma>=2', label: 'browse.chips.seen_2' },
        { token: 'karşılaşma>=3', label: 'browse.chips.seen_3' },
        { token: 'karşılaşma>=5', label: 'browse.chips.seen_5' },
        { token: 'karşılaşma=1', label: 'browse.chips.seen_1' },
      ],
    },
    {
      id: 'audio',
      label: 'browse.chips.audio',
      keys: ['ses', 'audio'],
      values: [
        { token: 'ses:var', label: 'browse.chips.media_yes' },
        { token: 'ses:yok', label: 'browse.chips.media_no' },
        { token: 'ses:video', label: 'browse.chips.media_video' },
        { token: 'ses:tts', label: 'browse.chips.audio_tts' },
      ],
    },
    {
      id: 'image',
      label: 'browse.chips.image',
      keys: ['gorsel', 'image'],
      values: [
        { token: 'görsel:var', label: 'browse.chips.media_yes' },
        { token: 'görsel:yok', label: 'browse.chips.media_no' },
        { token: 'görsel:video', label: 'browse.chips.media_video' },
      ],
    },
    { id: 'source', label: 'browse.chips.source', keys: ['kaynak', 'source'], values: [], text: 'kaynak:' },
    { id: 'word', label: 'browse.chips.word', keys: ['kelime', 'word'], module: 'words', values: [], text: 'kelime:' },
  ];
}

/** Folds like the Rust parser (`query::text::fold`): lower case, `ı`/`İ` → `i`, no accents. */
export function fold(s: string): string {
  return s
    .replace(/[Iıİ]/g, 'i')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '');
}

/** One whitespace-separated token of the query (quotes keep spaces inside). */
export interface Token {
  start: number;
  end: number;
  text: string;
  /** Folded key (`karsilasma`), `null` for a free word. */
  key: string | null;
  /** The value after the operator, quotes removed. */
  value: string;
  negated: boolean;
}

/** Splits `query` into top-level tokens (a quoted part never splits). */
export function tokens(query: string): Token[] {
  const out: Token[] = [];
  let i = 0;
  while (i < query.length) {
    while (i < query.length && /\s/.test(query[i]!)) i++;
    if (i >= query.length) break;
    const start = i;
    let quoted = false;
    while (i < query.length && (quoted || !/\s/.test(query[i]!))) {
      if (query[i] === '"') quoted = !quoted;
      i++;
    }
    const text = query.slice(start, i);
    const m = /^(-?)([^\s:<>="()-][^\s:<>="()]*)(:|>=|<=|!=|=|<|>)(.*)$/su.exec(text);
    out.push({
      start,
      end: i,
      text,
      key: m ? fold(m[2]!) : null,
      value: m ? m[4]!.replace(/^"(.*)"$/su, '$1') : '',
      negated: m ? m[1] === '-' : text.startsWith('-'),
    });
  }
  return out;
}

/** The first (non-negated) token of the query whose key is one of `keys`. */
export function findToken(query: string, keys: string[]): Token | null {
  return tokens(query).find((t) => !t.negated && t.key !== null && keys.includes(t.key)) ?? null;
}

/** Removes the spans and tidies the spaces they leave behind. */
function cut(query: string, spans: Token[], insert?: { at: Token; text: string }): string {
  let out = query;
  for (const s of [...spans].sort((a, b) => b.start - a.start)) {
    const text = insert && insert.at === s ? insert.text : '';
    out = out.slice(0, s.start) + text + out.slice(s.end);
  }
  return out.replace(/\s+/g, ' ').trim();
}

/**
 * Writes `token` for the chip with `keys`: replaces the first matching token in place (the
 * others with the same key go), or appends it at the end.
 */
export function setToken(query: string, keys: string[], token: string): string {
  const all = tokens(query).filter((t) => !t.negated && t.key !== null && keys.includes(t.key));
  if (all.length === 0) {
    const q = query.trim();
    return q ? `${q} ${token}` : token;
  }
  return cut(query, all, { at: all[0]!, text: token });
}

/** Deletes every (non-negated) token with one of `keys`. */
export function removeToken(query: string, keys: string[]): string {
  const all = tokens(query).filter((t) => !t.negated && t.key !== null && keys.includes(t.key));
  return all.length ? cut(query, all) : query;
}

/** The menu value a token stands for (same key family and folded text), if any. */
export function valueOf(def: ChipDef, token: Token): ChipValue | null {
  const text = fold(token.text);
  return def.values.find((v) => fold(v.token) === text) ?? null;
}
