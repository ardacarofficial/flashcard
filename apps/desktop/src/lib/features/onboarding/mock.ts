/**
 * Dev-only mock of the `onboarding` module (first-run guide + starter decks). Minimal: the
 * real plan and the starter deck contents live in fc-core (`ux::onboarding`, `ux::starter`).
 */
import { menu, module, err, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { Locale, OnboardingSubjectDto, StarterDeckDto, StudySubject } from '$lib/api/types';
import { CLASSIC_NOTETYPE_ID } from '$lib/api/client';

const S = (subject: StudySubject, goal: OnboardingSubjectDto['goal'], en: string, tr: string, template: string | null, starters: string[]) => ({
  subject,
  goal,
  en,
  tr,
  template,
  starters,
});

const SUBJECTS = [
  S('english', 'language', 'English', 'İngilizce', 'cefr', ['english_basics', 'app_guide']),
  S('japanese', 'language', 'Japanese', 'Japonca', 'jlpt', ['app_guide']),
  S('chinese', 'language', 'Chinese', 'Çince', 'hsk', ['app_guide']),
  S('other_language', 'language', 'Language', 'Dil', 'cefr', ['app_guide']),
  S('yks', 'exam', 'YKS', 'YKS', 'yks', ['yks_tyt_math', 'app_guide']),
  S('other_exam', 'exam', 'Exam', 'Sınav', 'difficulty', ['app_guide']),
  S('school', 'school', 'School', 'Okul', 'school_grade', ['app_guide']),
  S('general', 'general', 'General', 'Genel', 'difficulty', ['world_capitals', 'app_guide']),
];

const STARTERS = [
  { id: 'english_basics', en: 'Everyday English', tr: 'Günlük İngilizce', count: 24, template: 'cefr' },
  { id: 'yks_tyt_math', en: 'TYT math basics', tr: 'TYT matematik temelleri', count: 12, template: 'yks' },
  { id: 'world_capitals', en: 'World capitals', tr: 'Dünya başkentleri', count: 16, template: 'difficulty' },
  { id: 'app_guide', en: 'How Flashcard works', tr: 'Flashcard nasıl çalışır', count: 8, template: null },
];

export default {
  modules: [module('onboarding', [menu('onboarding.starters', 'nav.starters', '/starters', 94)])],
  handlers: ({ addDeck, decks, emit }: MockEnv) => {
    let completed = false;
    const lang = (l: Locale | null | undefined): Locale => l ?? 'en';
    const starter = (s: (typeof STARTERS)[number], l: Locale): StarterDeckDto => ({
      id: s.id,
      name: l === 'tr' ? s.tr : s.en,
      description: '',
      note_count: s.count,
      notetype_id: CLASSIC_NOTETYPE_ID,
      template_id: s.template,
    });
    const find = (sid: string) => {
      const s = STARTERS.find((x) => x.id === sid);
      if (!s) throw err('NOT_FOUND_STARTER_DECK', 'errors.not_found.starter_deck', sid);
      return s;
    };
    return {
      // Like the backend: a collection that already holds something is never interrupted.
      'onboarding.status': () => ({ show: !completed && decks.length === 0, completed }),
      'onboarding.options': ({ locale }) => ({
        subjects: SUBJECTS.map((s) => ({
          subject: s.subject,
          goal: s.goal,
          deck_name: lang(locale) === 'tr' ? s.tr : s.en,
          template_id: s.template,
          notetype_id: CLASSIC_NOTETYPE_ID,
          starters: s.starters,
        })),
        starters: STARTERS.map((s) => starter(s, lang(locale))),
      }),
      'onboarding.apply': ({ subject, deck_name, starters, locale }) => {
        const s = SUBJECTS.find((x) => x.subject === subject)!;
        const l = lang(locale);
        const deck = addDeck(deck_name?.trim() || (l === 'tr' ? s.tr : s.en), null);
        const ids = starters.map((sid) => addDeck(starter(find(sid), l).name, deck).id);
        completed = true;
        emit({ type: 'deck_created', data: { deck } });
        return {
          deck_id: deck.id,
          starter_deck_ids: ids,
          notes_created: starters.reduce((n, sid) => n + find(sid).count, 0),
        };
      },
      'onboarding.skip': () => {
        completed = true;
        return { show: false, completed };
      },
      'starter.list': ({ locale }) => ({ starters: STARTERS.map((s) => starter(s, lang(locale))) }),
      'starter.install': ({ id, locale }) => {
        const s = find(id);
        const deck = addDeck(starter(s, lang(locale)).name, null);
        emit({ type: 'deck_created', data: { deck } });
        return { deck_id: deck.id, notes_created: s.count };
      },
    };
  },
} satisfies MockFeature;
