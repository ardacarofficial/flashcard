import { mount } from 'svelte';
import './lib/theme/tokens.css';
import './lib/theme/base.css';
import App from './App.svelte';
import QuickAdd from './lib/features/inbox/QuickAdd.svelte';
import { theme } from './lib/theme/theme.svelte';
import { i18n } from './lib/i18n/index.svelte';

theme.init();
document.documentElement.lang = i18n.locale;

const target = document.getElementById('app');
if (!target) throw new Error('#app missing');

// The quick-add window (global shortcut, v0.9) loads the same page with `#quick-add`.
export default location.hash === '#quick-add' ? mount(QuickAdd, { target }) : mount(App, { target });
