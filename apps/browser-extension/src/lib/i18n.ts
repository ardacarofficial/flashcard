/**
 * All user-facing text comes from `_locales/<lang>/messages.json` through `chrome.i18n`
 * (TR/EN; the browser's UI language picks one, English is the fallback).
 */
export type MessageLookup = (key: string, substitutions?: string[]) => string;

let lookup: MessageLookup = (key, subs) =>
  typeof chrome !== 'undefined' && chrome.i18n ? chrome.i18n.getMessage(key, subs) : '';

/** Replaces the message source (tests). */
export function setMessageLookup(fn: MessageLookup): void {
  lookup = fn;
}

/** Translated text for `key`; the key itself when missing (a test keeps that from happening). */
export function t(key: string, substitutions?: string[]): string {
  const text = lookup(key, substitutions);
  return text === '' ? key : text;
}

/**
 * Fills `data-i18n` (text), `data-i18n-placeholder`, `data-i18n-title` and `data-i18n-aria`
 * attributes under `root`, and sets `<html lang>` from the UI language.
 */
export function localize(root: ParentNode): void {
  for (const el of root.querySelectorAll<HTMLElement>('[data-i18n]')) {
    el.textContent = t(el.dataset.i18n ?? '');
  }
  for (const el of root.querySelectorAll<HTMLElement>('[data-i18n-placeholder]')) {
    el.setAttribute('placeholder', t(el.dataset.i18nPlaceholder ?? ''));
  }
  for (const el of root.querySelectorAll<HTMLElement>('[data-i18n-title]')) {
    el.setAttribute('title', t(el.dataset.i18nTitle ?? ''));
  }
  for (const el of root.querySelectorAll<HTMLElement>('[data-i18n-aria]')) {
    el.setAttribute('aria-label', t(el.dataset.i18nAria ?? ''));
  }
  if (typeof document !== 'undefined' && typeof chrome !== 'undefined' && chrome.i18n) {
    document.documentElement.lang = chrome.i18n.getUILanguage().split('-')[0] ?? 'en';
  }
}
