<!--
  Keyboard shortcuts help host (v1.3): mounted once by the shell. `?` (not while typing, not
  over another dialog) opens the help sheet; so do the top bar's "?" and the palette, through
  `openHelp()`. The sheet's chunk loads on first open.
-->
<script lang="ts">
  import { setHelpHandler } from '$lib/shell/commands.svelte';
  import { typingTarget } from '../browse/palette';

  interface Props {
    /** Opens the Shortcuts screen; `null` when it is not there (the `palette` module is off). */
    openShortcuts: (() => void) | null;
  }
  let { openShortcuts }: Props = $props();

  let open = $state(false);
  /** Opened at least once: the sheet's chunk is loaded and stays mounted. */
  let used = $state(false);

  function show() {
    used = true;
    open = true;
  }

  $effect(() => setHelpHandler(show));

  /** `?` is fixed (not in the shortcut table): typing it in a field types it. */
  function onkeydown(e: KeyboardEvent) {
    if (e.key !== '?' || e.ctrlKey || e.metaKey || e.altKey || e.repeat || e.defaultPrevented) return;
    if (typingTarget(e.target)) return;
    if (document.querySelector('[aria-modal="true"]')) return;
    e.preventDefault();
    show();
  }
</script>

<svelte:window {onkeydown} />

{#if used}
  {#await import('./HelpSheet.svelte') then { default: HelpSheet }}
    <HelpSheet bind:open {openShortcuts} />
  {/await}
{/if}
