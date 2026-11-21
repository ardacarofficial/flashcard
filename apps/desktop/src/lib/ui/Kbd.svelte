<!--
  Keyboard shortcut hint: `<Kbd keys={['Mod', 'K']} />`. `Mod` shows ⌘ on macOS and Ctrl
  elsewhere; also accepts a single combo string (`"Mod+Enter"`).
-->
<script lang="ts">
  interface Props {
    keys: string[] | string;
    /** `sm` inside buttons and fields (18 px), `md` elsewhere (20 px). */
    size?: 'sm' | 'md';
    class?: string;
  }

  let { keys, size = 'md', class: klass = '' }: Props = $props();

  const mac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform ?? '');
  const NAMES: Record<string, string> = mac
    ? { Mod: '⌘', Alt: '⌥', Shift: '⇧', Ctrl: '⌃', Enter: '↵', ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→' }
    : { Mod: 'Ctrl', Enter: '↵', ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→', ' ': 'Space' };
  const list = $derived((Array.isArray(keys) ? keys : keys.split('+')).map((k) => NAMES[k] ?? k));
</script>

<span class="fc-kbd {size} {klass}">
  {#each list as k, i (i)}<kbd>{k}</kbd>{/each}
</span>

<style>
  .fc-kbd {
    display: inline-flex;
    gap: 2px;
    vertical-align: middle;
  }
  kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 20px;
    padding: 0 5px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
    line-height: 1;
    white-space: nowrap;
  }
  .sm kbd {
    min-width: 18px;
    height: 18px;
    padding: 0 4px;
    font-size: 10.5px;
  }
</style>
