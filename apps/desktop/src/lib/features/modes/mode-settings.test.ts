import { describe, expect, it } from 'vitest';
import type { SettingSchemaEntry } from '$lib/api/types';
import { modeSettingSections } from './mode-settings';

const entry = (key: string, advanced = false): SettingSchemaEntry => ({
  module: key.split('.')[0]!,
  def: { key, kind: { type: 'bool' }, default: false, label_key: `settings.${key}.label`, description_key: null, advanced, scopes: ['app'] },
});

describe('Settings → Reading and modes', () => {
  it('groups the enabled modules’ settings by section, in prefix order, without advanced ones', () => {
    const schema = [
      entry('reading.ffmpeg_path', true),
      entry('focus.auto'),
      entry('reading.default_language'),
      entry('study.type_answer'),
      entry('study.answer_buttons'),
      entry('practice_test.questions'),
      entry('learn.max_cards'),
      entry('match.best_ms', true),
    ];
    expect(modeSettingSections(schema).map((s) => [s.id, s.defs.map((d) => d.key)])).toEqual([
      ['study', ['study.answer_buttons', 'focus.auto']],
      ['practice', ['learn.max_cards', 'practice_test.questions']],
      ['reading', ['reading.default_language']],
    ]);
  });

  it('drops a section whose modules are all off', () => {
    expect(modeSettingSections([entry('learn.max_cards')]).map((s) => s.id)).toEqual(['practice']);
    expect(modeSettingSections([])).toEqual([]);
  });
});
