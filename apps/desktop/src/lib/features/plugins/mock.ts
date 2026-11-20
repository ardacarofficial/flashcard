/**
 * Dev-only mock of the plugins and tts modules (see `api/mock/transport.ts`): one installed
 * demo plugin with a command and a tiny study mode. No sandbox, no permission checks: the
 * real rules live in fc-app `modules/plugins` and fc-plugins.
 */
import { menu, module, err, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { PluginDto, UiNode, UiSession } from '$lib/api/types';

const DEMO = 'com.example.demo';

export default {
  modules: [module('plugins', [menu('plugins.manager', 'nav.plugins', '/plugins', 92)]), module('tts', [])],
  handlers: ({ now, emit }: MockEnv) => {
    const t0 = now();
    const plugins: PluginDto[] = [
      {
        id: DEMO,
        name: 'Demo',
        version: '1.0.0',
        description: 'A demo plugin of the mock backend.',
        authors: ['Flashcard'],
        api_version: '1',
        enabled: true,
        status: 'active',
        failures: 0,
        last_error: null,
        permissions: [
          { permission: 'ui:command', granted: true, sensitive: false },
          { permission: 'ui:study_mode', granted: true, sensitive: false },
          { permission: 'network:example.com', granted: false, sensitive: true },
        ],
        contributes: {
          commands: ['hello'],
          study_modes: ['quiz'],
          views: [],
          card_types: [],
          tts: [],
          menu_items: [],
          sidebar_panels: [],
          card_toolbar: [],
          hooks: [],
        },
        has_settings: true,
        installed_at: t0,
        updated_at: t0,
      },
    ];
    const settings: Record<string, unknown> = { greeting: 'Merhaba' };
    const sessions = new Map<string, number>();
    let seq = 0;
    const find = (id: string) => {
      const p = plugins.find((x) => x.id === id);
      if (!p) throw err('NOT_FOUND_PLUGIN', 'errors.not_found.plugin', id);
      return p;
    };
    const changed = (id: string) => emit({ type: 'plugins_changed', data: { plugin_id: id, auto_disabled: false, reason: null } });
    const quiz = (step: number, correct: boolean | null): UiNode => ({
      kind: 'column',
      children: [
        ...(correct === null ? [] : [{ kind: 'feedback', correct, text: { key: correct ? 'right' : 'wrong', params: {} } } as UiNode]),
        { kind: 'progress', value: step, max: 3 },
        { kind: 'heading', text: { text: ['sword', 'shield', 'book'][step % 3]! } },
        { kind: 'input', id: 'answer', label: { key: 'answer', params: {} }, value: null, multiline: false },
        { kind: 'button', action: 'check', label: { key: 'check', params: {} }, primary: true },
      ],
    });
    const session = (session_id: string, node: UiNode | null, done: boolean): UiSession => ({
      session_id,
      plugin_id: DEMO,
      view: 'quiz',
      title: { key: 'title', params: {} },
      node,
      done,
      notices: [],
    });
    return {
      'plugins.list': () => ({ plugins }),
      'plugins.locales': ({ lang }) => ({
        plugins: {
          [DEMO]:
            lang === 'tr'
              ? { name: 'Demo', 'commands.hello': 'Selam ver', title: 'Mini sınav', answer: 'Cevabın', check: 'Kontrol et', right: 'Doğru!', wrong: 'Yanlış', 'settings.greeting': 'Selam metni' }
              : { name: 'Demo', 'commands.hello': 'Say hello', title: 'Mini quiz', answer: 'Your answer', check: 'Check', right: 'Right!', wrong: 'Wrong', 'settings.greeting': 'Greeting' },
        },
      }),
      'plugins.inspect': ({ path }) => {
        throw err('VALIDATION_PLUGIN_PACKAGE', 'errors.validation.plugin_package', path);
      },
      'plugins.install': ({ path }) => {
        throw err('VALIDATION_PLUGIN_PACKAGE', 'errors.validation.plugin_package', path);
      },
      'plugins.uninstall': ({ id }) => {
        const p = find(id);
        plugins.splice(plugins.indexOf(p), 1);
        changed(id);
        return p;
      },
      'plugins.set_enabled': ({ id, enabled }) => {
        const p = find(id);
        Object.assign(p, { enabled, status: enabled ? 'active' : 'disabled', updated_at: now() });
        changed(id);
        return p;
      },
      'plugins.set_permissions': ({ id, granted }) => {
        const p = find(id);
        p.permissions = p.permissions.map((x) => ({ ...x, granted: granted.includes(x.permission) }));
        changed(id);
        return p;
      },
      'plugins.settings.get': ({ id }) => ({
        id: find(id).id,
        schema: [
          {
            key: 'greeting',
            kind: { type: 'text', max_len: 40 },
            default: 'Merhaba',
            label_key: 'settings.greeting',
            description_key: null,
            advanced: false,
            scopes: ['plugin'],
          },
        ],
        values: { ...settings },
      }),
      'plugins.settings.set': ({ id, key, value }) => {
        settings[key] = value ?? 'Merhaba';
        return {
          id: find(id).id,
          schema: [],
          values: { ...settings },
        };
      },
      'plugins.logs': ({ id }) => ({ entries: [{ at: now(), level: 'info', message: `${find(id).id} started` }] }),
      'plugins.run': ({ plugin_id }) => ({
        result: String(settings.greeting),
        notices: [{ plugin_id, level: 'info', text: { text: String(settings.greeting) } }],
      }),
      'plugins.ui.open': ({ plugin_id }) => {
        find(plugin_id);
        const id = `s${++seq}`;
        sessions.set(id, 0);
        return session(id, quiz(0, null), false);
      },
      'plugins.ui.act': ({ session_id, values }) => {
        const step = sessions.get(session_id);
        if (step === undefined) throw err('NOT_FOUND_PLUGIN_SESSION', 'errors.not_found.plugin_session', session_id);
        const expected = ['kılıç', 'kalkan', 'kitap'][step % 3];
        const correct = String(values.answer ?? '').trim().toLowerCase() === expected;
        if (step >= 2) {
          sessions.delete(session_id);
          return session(session_id, null, true);
        }
        sessions.set(session_id, step + 1);
        return session(session_id, quiz(step + 1, correct), false);
      },
      'plugins.ui.close': ({ session_id }) => {
        sessions.delete(session_id);
        return {};
      },
      'plugins.models.list': () => ({ models: [], disk_usage: 0 }),
      'plugins.models.download': ({ plugin_id }) => {
        throw err('NOT_FOUND_PLUGIN_CONTRIBUTION', 'errors.not_found.plugin_contribution', plugin_id);
      },
      'plugins.models.delete': ({ plugin_id }) => {
        throw err('NOT_FOUND_PLUGIN_CONTRIBUTION', 'errors.not_found.plugin_contribution', plugin_id);
      },
      'tts.providers': () => ({ providers: [] }),
      'tts.voices': () => {
        throw err('PROVIDER_UNAVAILABLE', 'errors.provider.unavailable', 'no TTS provider');
      },
      'tts.synthesize': () => {
        throw err('PROVIDER_UNAVAILABLE', 'errors.provider.unavailable', 'no TTS provider');
      },
    };
  },
} satisfies MockFeature;
