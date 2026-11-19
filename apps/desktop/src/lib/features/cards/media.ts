/** Media transport for the UI: uploading files and turning `media:<hash>` into URLs. */
import { api } from '$lib/api';
import { isTauri } from '$lib/api/transport';
import type { MediaDto } from '$lib/api/types';
import { mapMediaSrc, mediaHashes } from './content';

/** Reads a file as standard base64 (no data-URL prefix). */
export function fileToBase64(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error('read failed'));
    reader.onload = () => {
      const s = String(reader.result);
      resolve(s.slice(s.indexOf(',') + 1));
    };
    reader.readAsDataURL(file);
  });
}

/** Stores a file in the media store (deduplicated by content in the backend). */
export async function uploadMedia(file: File): Promise<MediaDto> {
  return api.call('media.add', { name: file.name || null, data_base64: await fileToBase64(file) });
}

const cache = new Map<string, string>();

/** URL of a media file: the `fcmedia` protocol under Tauri, else a data URL via `media.read`. */
async function urlOf(hash: string): Promise<string> {
  const hit = cache.get(hash);
  if (hit) return hit;
  let url: string;
  if (isTauri()) {
    const { convertFileSrc } = await import('@tauri-apps/api/core');
    url = convertFileSrc(hash, 'fcmedia');
  } else {
    const m = await api.call('media.read', { hash });
    url = `data:${m.mime};base64,${m.data_base64}`;
  }
  cache.set(hash, url);
  return url;
}

/** Rendered HTML with every media reference replaced by a loadable URL. */
export async function resolveMedia(html: string): Promise<string> {
  const urls = new Map<string, string>();
  for (const h of mediaHashes(html)) {
    try {
      urls.set(h, await urlOf(h));
    } catch {
      urls.set(h, '');
    }
  }
  return mapMediaSrc(html, (h) => urls.get(h) ?? '');
}
