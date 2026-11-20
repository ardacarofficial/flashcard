<!--
  A Lucide icon (see icons.ts). Decorative by default (`aria-hidden`); pass `label` when the
  icon alone carries meaning.
-->
<script lang="ts">
  import type { IconNode } from 'lucide';

  interface Props {
    icon: IconNode;
    /** Pixel size (16 inline, 20 controls, 24 navigation). */
    size?: number;
    strokeWidth?: number;
    /** Accessible name; omit for decorative icons. */
    label?: string;
    class?: string;
  }

  let { icon, size = 16, strokeWidth = 2, label, class: klass = '' }: Props = $props();

  type Child = readonly [string, Record<string, string | number>];
  const esc = (v: string | number) => String(v).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');

  /** Lucide nodes are `[tag, attrs, children]` (lucide ≤ 0.4x) or a plain children list. */
  const markup = $derived.by(() => {
    const children = (typeof icon[0] === 'string' ? (icon[2] ?? []) : icon) as readonly Child[];
    // Static icon data from the bundled set (never user input), escaped all the same.
    return children
      .map(([tag, attrs]) => `<${tag} ${Object.entries(attrs).map(([k, v]) => `${k}="${esc(v)}"`).join(' ')}/>`)
      .join('');
  });
</script>

<svg
  class="fc-icon {klass}"
  xmlns="http://www.w3.org/2000/svg"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={strokeWidth}
  stroke-linecap="round"
  stroke-linejoin="round"
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
  focusable="false"
>
  {@html markup}
</svg>

<style>
  .fc-icon {
    flex: none;
    display: inline-block;
    vertical-align: middle;
  }
</style>
