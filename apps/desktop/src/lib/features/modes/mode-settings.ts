import type { SettingSchemaEntry } from '$lib/api/types';

/** Sections of Settings → "Reading and modes" and the setting-key prefixes each one shows. */
export const MODE_SETTING_SECTIONS = [
  { id: 'study', title_key: 'mode_settings.study', prefixes: ['study.answer_buttons', 'confidence.', 'timed.', 'focus.'] },
  { id: 'practice', title_key: 'mode_settings.practice', prefixes: ['learn.', 'practice_test.', 'match.'] },
  { id: 'reading', title_key: 'mode_settings.reading', prefixes: ['reading.'] },
] as const;

/**
 * Groups the schema (`settings.schema`: settings of enabled modules only) into the page's
 * sections, in section-prefix order; empty sections are dropped. Advanced settings stay out.
 */
export function modeSettingSections(schema: SettingSchemaEntry[]) {
  return MODE_SETTING_SECTIONS.map((s) => ({
    ...s,
    defs: s.prefixes.flatMap((p) =>
      schema.filter((e) => !e.def.advanced && (p.endsWith('.') ? e.def.key.startsWith(p) : e.def.key === p)).map((e) => e.def),
    ),
  })).filter((s) => s.defs.length > 0);
}
