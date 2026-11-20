<!--
  Keyboard shortcut hint: `<Kbd keys={['Mod', 'K']} />`. `Mod` shows ⌘ on macOS and Ctrl
  elsewhere; also accepts a single combo string (`"Mod+Enter"`).
-->
<script lang="ts">
  interface Props {
    keys: string[] | string;
    class?: string;
  }

  let { keys, class: klass = '' }: Props = $props();

  const mac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform ?? '');
  const NAMES: Record<string, string> = mac
    ? { Mod: '⌘', Alt: '⌥', Shift: '⇧', Ctrl: '⌃', Enter: '↵', ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→' }
    : { Mod: 'Ctrl', Enter: '↵', ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→', ' ': 'Space' };
  const list = $derived((Array.isArray(keys) ? keys : keys.split('+')).map((k) => NAMES[k] ?? k));
</script>

<span class="fc-kbd {klass}">
  {#each list as k, i (i)}<kbd>{k}</kbd>{/each}
</span>

<style>
  .fc-kbd {
    display: inline-flex;
    gap: 2px;
  }
  kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 20px;
    padding: 0 5px;
    border: 1px solid var(--border-strong);
    border-bottom-width: 2px;
    border-radius: 4px;
    background: var(--surface-1);
    color: var(--text-2);
    font-family: var(--font-sans);
    font-size: 11px;
    font-weight: var(--weight-medium);
    line-height: 1;
  }
</style>
