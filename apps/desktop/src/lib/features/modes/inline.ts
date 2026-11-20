/**
 * Dev-mock only: a tiny `question :: answer` splitter so the mock backend can preview inline
 * cards. The real parser (cloze, links, code, escapes, list markers) is
 * `fc_core::inline_cards`; the screens always ask the backend (`modes.inline.preview`).
 */
export function parseInline(text: string): { line: number; front: string; back: string }[] {
  return text.split(/\r?\n/).flatMap((raw, i) => {
    const line = raw.replace(/^\s*(?:[-*+•]|\d+[.)])\s+/, '');
    const at = line.indexOf('::');
    if (at < 0 || line.includes('{{')) return [];
    const front = line.slice(0, at).trim();
    const back = line.slice(at + 2).trim();
    return front && back ? [{ line: i + 1, front, back }] : [];
  });
}
