import type { FlagDto } from '$lib/api/types';

/** Bit of flag `n` (1–7) in `cards.flags`. */
export const flagBit = (n: number): number => 1 << (n - 1);

/** Whether `flags` has flag `n`. */
export const hasFlag = (flags: number, n: number): boolean => (flags & flagBit(n)) !== 0;

/** i18n key of a flag's default name. */
export const defaultFlagKey = (n: number): string => `flags.default.${n}`;

/** CSS color of a flag's color token (tokens are plain CSS color names or `#hex`). */
export const flagColor = (f: Pick<FlagDto, 'color'>): string => f.color;
