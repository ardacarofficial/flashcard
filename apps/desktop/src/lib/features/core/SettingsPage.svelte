<script lang="ts">
  import SettingsScreen from '$lib/components/SettingsScreen.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx: _ctx }: { ctx: ScreenCtx } = $props();
</script>

<SettingsScreen />
