import ProfilesScreen from './ProfilesScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'profiles', component: ProfilesScreen, routes: ['/profiles'] }] satisfies ScreenModule;
