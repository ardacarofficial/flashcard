<!--
  Study launcher host (v1.3): mounted once by the shell. While the study module is on it
  registers `openLauncher` (every "study" entry calls it), loads the dialog's own chunk on
  first open, and takes a started or continued session to its mode's screen.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { moduleKnownOn, trackModules } from '../cards/modules.svelte';
  import type { ScreenCtx } from '../types';
  import { handOff, KIND_ROUTE, setLauncherHandler, type Handoff, type LaunchRequest } from './launcher.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let open = $state(false);
  let request = $state<LaunchRequest>({ deckId: null });
  /** Opened at least once: the dialog's chunk is loaded and stays mounted. */
  let used = $state(false);

  onMount(() => trackModules());

  const enabled = $derived(moduleKnownOn('study'));

  $effect(() => {
    if (!enabled) {
      open = false;
      return;
    }
    return setLauncherHandler((r) => {
      request = { ...r };
      used = true;
      open = true;
    });
  });

  function onstart(h: Handoff) {
    handOff(h);
    ctx.selectDeck(h.session.deck_id);
    ctx.navigate?.(KIND_ROUTE[h.kind]);
  }
</script>

{#if used && enabled}
  {#await import('./StudyLauncher.svelte') then { default: StudyLauncher }}
    <StudyLauncher bind:open {request} {onstart} />
  {/await}
{/if}
