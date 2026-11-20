import { isTauri } from '$lib/api/transport';

/** A file type filter of the native dialogs. */
export interface FileFilter {
  name: string;
  extensions: string[];
}

/**
 * Asks for a file to open. Native dialog in the app; in the browser dev mock (no file
 * system) a plain prompt for a path.
 */
export async function pickFile(filters: FileFilter[], promptLabel: string): Promise<string | null> {
  if (isTauri()) {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({ multiple: false, directory: false, filters });
    return typeof picked === 'string' ? picked : null;
  }
  return window.prompt(promptLabel) || null;
}

/** Asks where to save a file (`defaultName` pre-filled). */
export async function pickSavePath(
  defaultName: string,
  filters: FileFilter[],
  promptLabel: string,
): Promise<string | null> {
  if (isTauri()) {
    const { save } = await import('@tauri-apps/plugin-dialog');
    return (await save({ defaultPath: defaultName, filters })) ?? null;
  }
  return window.prompt(promptLabel, defaultName) || null;
}

/** File name safe on every OS. */
export function safeName(name: string): string {
  return name.replace(/[\\/:*?"<>|]+/g, '_').trim() || 'flashcard';
}
