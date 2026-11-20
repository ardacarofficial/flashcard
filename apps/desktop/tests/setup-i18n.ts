// Tests render screens synchronously: load both dictionaries up front (the app loads them lazily).
import { loadLocale } from '../src/lib/i18n/core';

await Promise.all([loadLocale('en'), loadLocale('tr')]);
