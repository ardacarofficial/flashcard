/** Speaks a card's `{{tts:…}}` span (study, previews). */
import { api } from '$lib/api';
import { moduleOn } from './modules.svelte';

/** The audio element used to play synthesized speech (replaceable in tests). */
export const player = { play: (src: string): Promise<void> => new Audio(src).play() };

/**
 * Through the backend's TTS providers (`tts.synthesize`: plugin providers, `tts.provider`
 * setting) when the `tts` module is on and a provider is registered; otherwise, or when the
 * provider fails, the webview's own speech synthesis. Returns which one spoke.
 */
export async function speak(text: string, lang: string): Promise<'provider' | 'webview' | 'none'> {
  if (moduleOn('tts')) {
    try {
      const { providers } = await api.call('tts.providers', {});
      if (providers.length > 0) {
        const audio = await api.call('tts.synthesize', { text, language: lang || 'und' });
        await player.play(`data:${audio.mime};base64,${audio.data_b64}`);
        return 'provider';
      }
    } catch {
      /* fall back to the webview */
    }
  }
  if (typeof speechSynthesis === 'undefined') return 'none';
  const u = new SpeechSynthesisUtterance(text);
  if (lang) u.lang = lang;
  speechSynthesis.speak(u);
  return 'webview';
}
