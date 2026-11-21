<!--
  Quick Add host (v1.2): mounted once by the shell. Registers itself as the shell's "add a card"
  (top bar button, `N`, `Ctrl+N`, the palette — `openQuickAdd()`) while the notes module is on,
  and loads the dialog's own chunk only on first open (bundle budget).
-->
<script lang="ts">
  import { setQuickAddHandler } from '$lib/shell/commands.svelte';
  import type { ScreenCtx } from '../types';

  interface Props {
    ctx: ScreenCtx;
    /** The notes module is on (otherwise the shell's fallback stays in charge). */
    enabled: boolean;
  }
  let { ctx, enabled }: Props = $props();

  let open = $state(false);
  /** Opened at least once: the dialog's chunk is loaded and stays mounted. */
  let used = $state(false);

  $effect(() => {
    if (!enabled) {
      open = false;
      return;
    }
    return setQuickAddHandler(() => {
      used = true;
      open = true;
    });
  });
</script>

{#if used && enabled}
  {#await import('./QuickAddDialog.svelte') then { default: QuickAddDialog }}
    <QuickAddDialog bind:open {ctx} />
  {/await}
{/if}
