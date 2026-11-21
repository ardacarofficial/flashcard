<script lang="ts">
  import { SegmentedControl } from '$lib/ui';
  import { t } from '$lib/i18n/index.svelte';
  import { firstMediaHash, maskFromDrag, parseMasks, serializeMasks, type Masks } from './content';
  import { resolveMedia } from './media';

  interface Props {
    /** The `masks` field (JSON). */
    value: string;
    /** The image field's value (`media:<hash>`). */
    image: string;
    label: string;
    required?: boolean;
  }
  let { value = $bindable(), image, label, required = false }: Props = $props();

  const masks = $derived<Masks>(parseMasks(value));
  let src = $state('');
  let box = $state<HTMLElement | null>(null);
  let start = $state<{ x: number; y: number } | null>(null);
  let draft = $state<{ x: number; y: number; w: number; h: number } | null>(null);

  $effect(() => {
    const hash = firstMediaHash(image);
    let cancelled = false;
    if (!hash) {
      src = '';
      return;
    }
    void resolveMedia(`src="media:${hash}"`).then((s) => {
      if (!cancelled) src = /src="([^"]*)"/.exec(s)?.[1] ?? '';
    });
    return () => {
      cancelled = true;
    };
  });

  function point(e: PointerEvent) {
    const r = box!.getBoundingClientRect();
    return { x: (e.clientX - r.left) / r.width, y: (e.clientY - r.top) / r.height };
  }

  function down(e: PointerEvent) {
    if (!box || (e.target as HTMLElement).closest('.mask')) return;
    box.setPointerCapture(e.pointerId);
    start = point(e);
  }

  function moveTo(e: PointerEvent) {
    if (!start) return;
    const p = point(e);
    draft = {
      x: Math.min(start.x, p.x),
      y: Math.min(start.y, p.y),
      w: Math.abs(p.x - start.x),
      h: Math.abs(p.y - start.y),
    };
  }

  function up(e: PointerEvent) {
    if (!start) return;
    const p = point(e);
    const m = maskFromDrag(masks.masks, start.x, start.y, p.x, p.y);
    start = null;
    draft = null;
    if (m) value = serializeMasks({ ...masks, masks: [...masks.masks, m] });
  }

  function remove(n: number) {
    value = serializeMasks({ ...masks, masks: masks.masks.filter((m) => m.n !== n) });
  }

  function setMode(mode: Masks['mode']) {
    value = serializeMasks({ ...masks, mode });
  }

  /** A 0–1 fraction as a CSS length (`calc(0.25 * 100%)`), not display text. */
  const pct = (v: number) => `calc(${v} * 100%)`;
</script>

<div class="masks">
  <div class="head">
    <span class="label">{label}{#if required}<span class="req" aria-hidden="true">*</span>{/if}</span>
    {#if src}<span class="muted num">{t('masks.count', { n: masks.masks.length })}</span>{/if}
  </div>
  {#if !src}
    <p class="muted empty">{t('masks.need_image')}</p>
  {:else}
    <SegmentedControl
      size="sm"
      label={t('masks.mode')}
      options={[
        { value: 'hide_all', label: t('masks.hide_all') },
        { value: 'hide_one', label: t('masks.hide_one') },
      ]}
      value={masks.mode}
      onchange={(v) => setMode(v as Masks['mode'])}
    />
    <div
      class="canvas"
      bind:this={box}
      role="application"
      aria-label={t('masks.draw_hint')}
      onpointerdown={down}
      onpointermove={moveTo}
      onpointerup={up}
    >
      <img {src} alt="" draggable="false" />
      {#each masks.masks as m (m.n)}
        <button
          type="button"
          class="mask"
          style:left={pct(m.x)}
          style:top={pct(m.y)}
          style:width={pct(m.w)}
          style:height={pct(m.h)}
          title={t('masks.remove')}
          aria-label={t('masks.remove_n', { n: m.n })}
          onclick={() => remove(m.n)}
        >
          {m.n}
        </button>
      {/each}
      {#if draft}
        <div class="draft" style:left={pct(draft.x)} style:top={pct(draft.y)} style:width={pct(draft.w)} style:height={pct(draft.h)}></div>
      {/if}
    </div>
    <span class="muted">{t('masks.draw_hint')}</span>
  {/if}
</div>

<style>
  .masks {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
    align-self: stretch;
  }
  .label {
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .req {
    color: var(--danger);
    margin-inline-start: 2px;
  }
  .muted {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .num {
    font-family: var(--font-mono);
  }
  .empty {
    align-self: stretch;
    padding: var(--space-3);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-control);
  }
  .canvas {
    position: relative;
    display: inline-block;
    max-width: 100%;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    overflow: hidden;
    cursor: crosshair;
    user-select: none;
    touch-action: none;
  }
  .canvas img {
    display: block;
    max-width: 100%;
  }
  .mask,
  .draft {
    position: absolute;
    border-radius: 2px;
  }
  .mask {
    background: var(--accent);
    color: var(--accent-fg);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    opacity: 0.9;
    padding: 0;
    min-height: 0;
    border: none;
    cursor: pointer;
  }
  .mask:hover {
    background: var(--danger-solid);
    color: var(--danger-solid-fg);
  }
  .mask:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .draft {
    outline: 2px dashed var(--accent);
  }
</style>
