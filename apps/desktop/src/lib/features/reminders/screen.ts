import RemindersScreen from './RemindersScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'reminders', component: RemindersScreen, routes: ['/reminders'] }] satisfies ScreenModule;
