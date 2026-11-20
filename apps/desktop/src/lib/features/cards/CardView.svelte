<script lang="ts">
  import 'katex/dist/katex.min.css';
  import type { RenderedSides } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { scopeCss } from './content';
  import { resolveMedia } from './media';
  import { moduleOn } from './modules.svelte';
  import { enhance } from './rich';

  interface Props {
    sides: RenderedSides;
    showAnswer?: boolean;
    /** Called when a `[[link]]` to another note is clicked. */
    onopennote?: (id: string) => void;
    /** Draw the card's own answer controls (choices, typing box). Off when the host (the
     * study screen) owns answering. */
    interactive?: boolean;
    /** `data-testid` of the card body. */
    testid?: string;
  }
  let { sides, showAnswer = false, onopennote, interactive = true, testid }: Props = $props();

  const scope = `fc-card-${Math.random().toString(36).slice(2, 10)}`;
  let el = $state<HTMLElement | null>(null);
  let html = $state('');
  let picked = $state<number | null>(null);
  let typed = $state('');

  $effect(() => {
    const src = showAnswer ? sides.answer : sides.question;
    let cancelled = false;
    void resolveMedia(src).then((h) => {
      if (!cancelled) html = h;
    });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    void html;
    if (el) void enhance(el, { math: moduleOn('math'), code: moduleOn('code_highlight'), hintLabel: t('cards.hint') });
  });

  $effect(() => {
    void sides;
    picked = null;
    typed = '';
  });

  function onclick(e: MouseEvent) {
    const a = (e.target as HTMLElement | null)?.closest('a');
    if (!a) return;
    // Cards never navigate the app window; note links open the note.
    e.preventDefault();
    const href = a.getAttribute('href') ?? '';
    if (href.startsWith('note:')) onopennote?.(href.slice(5));
  }

  function speak(text: string, lang: string) {
    if (typeof speechSynthesis === 'undefined') return;
    const u = new SpeechSynthesisUtterance(text);
    if (lang) u.lang = lang;
    speechSynthesis.speak(u);
  }
</script>

{@html `<style>${scopeCss(sides.css, scope)}</style>`}
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="card {scope}" bind:this={el} {onclick} data-testid={testid}>
  {@html html}
</div>

{#if !interactive}
  <!-- answering is the host's -->
{:else if sides.interaction.kind === 'choice' && !showAnswer}
  <div class="choices">
    {#each sides.interaction.options as option, i (i)}
      <button
        class:right={picked !== null && i === sides.interaction.correct}
        class:wrong={picked === i && i !== sides.interaction.correct}
        onclick={() => (picked = i)}
      >
        {@html option}
      </button>
    {/each}
  </div>
{:else if sides.interaction.kind === 'type'}
  {#if showAnswer}
    <p class="expected"><span class="muted">{t('cards.expected')}:</span> {sides.interaction.expected}</p>
  {:else}
    <input class="type-box" bind:value={typed} placeholder={t('cards.type_placeholder')} aria-label={t('cards.type_placeholder')} />
  {/if}
{/if}

{#if sides.tts.length > 0}
  <div class="tts">
    {#each sides.tts as item, i (i)}
      <button class="small" onclick={() => speak(item.text, item.lang)} aria-label={t('cards.speak')}>🔊 {item.text}</button>
    {/each}
  </div>
{/if}

<style>
  .card {
    background: var(--fc-surface);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-4);
    min-height: 4rem;
    overflow-wrap: anywhere;
  }
  .card :global(img),
  .card :global(video) {
    max-width: 100%;
  }
  .card :global(.cloze) {
    font-weight: 600;
    color: var(--fc-accent);
  }
  .card :global(.fc-link-missing) {
    text-decoration: underline dotted;
    color: var(--fc-text-muted);
  }
  .card :global(.fc-io) {
    position: relative;
    display: inline-block;
  }
  .card :global(.fc-io img) {
    display: block;
  }
  .card :global(.fc-io-mask) {
    position: absolute;
    background: var(--fc-accent);
    opacity: 0.85;
    border-radius: 2px;
  }
  .card :global(.fc-io-active) {
    background: var(--fc-danger);
  }
  .card :global(.fc-io-revealed) {
    background: transparent;
    outline: 2px solid var(--fc-danger);
  }
  .card :global(pre) {
    background: var(--fc-bg-subtle);
    padding: var(--fc-space-2);
    border-radius: var(--fc-radius);
    overflow-x: auto;
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    margin-top: var(--fc-space-3);
  }
  .choices button {
    text-align: left;
  }
  .choices .right {
    border-color: var(--fc-success);
  }
  .choices .wrong {
    border-color: var(--fc-danger);
  }
  .type-box {
    margin-top: var(--fc-space-3);
    width: 100%;
  }
  .expected {
    margin-top: var(--fc-space-3);
  }
  .tts {
    display: flex;
    gap: var(--fc-space-2);
    margin-top: var(--fc-space-2);
  }
</style>
