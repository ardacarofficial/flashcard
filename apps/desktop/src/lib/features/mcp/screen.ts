import McpScreen from './McpScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'mcp', component: McpScreen, routes: ['/mcp'] }] satisfies ScreenModule;
