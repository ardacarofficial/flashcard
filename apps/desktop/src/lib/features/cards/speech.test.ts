import { beforeEach, describe, expect, it, vi } from 'vitest';

const call = vi.fn();
let ttsOn = true;
vi.mock('$lib/api', () => ({ api: { call: (m: string, p: unknown) => call(m, p) } }));
vi.mock('./modules.svelte', () => ({ moduleOn: (id: string) => (id === 'tts' ? ttsOn : true) }));

import { player, speak } from './speech';

describe('speak', () => {
  beforeEach(() => {
    call.mockReset();
    ttsOn = true;
  });

  it('uses a plugin TTS provider when one is registered', async () => {
    const play = vi.spyOn(player, 'play').mockResolvedValue();
    call.mockImplementation(async (m: string) =>
      m === 'tts.providers'
        ? { providers: [{ id: 'plugin.com.example.tone.tone', name_key: 'x', languages: [], local: true }] }
        : { provider: 'plugin.com.example.tone.tone', mime: 'audio/wav', data_b64: 'UklGRg==' },
    );
    expect(await speak('kılıç', 'tr')).toBe('provider');
    expect(call).toHaveBeenCalledWith('tts.synthesize', { text: 'kılıç', language: 'tr' });
    expect(play).toHaveBeenCalledWith('data:audio/wav;base64,UklGRg==');
  });

  it('falls back to the webview without a provider or with the module off', async () => {
    call.mockResolvedValue({ providers: [] });
    expect(await speak('run', 'en')).not.toBe('provider');
    expect(call).not.toHaveBeenCalledWith('tts.synthesize', expect.anything());
    ttsOn = false;
    call.mockReset();
    expect(await speak('run', 'en')).not.toBe('provider');
    expect(call).not.toHaveBeenCalled();
  });
});
