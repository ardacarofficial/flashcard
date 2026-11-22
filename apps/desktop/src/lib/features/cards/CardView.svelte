<script lang="ts">
  import 'katex/dist/katex.min.css';
  import type { RenderedSides } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { scopeCss } from './content';
  import { resolveMedia } from './media';
  import { moduleOn } from './modules.svelte';
  import { enhance } from './rich';
  import { speak } from './speech';
  import { Icon, Play } from '$lib/ui';

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
        type="button"
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
      <button type="button" class="speak" onclick={() => void speak(item.text, item.lang)} aria-label={`${t('cards.speak')}: ${item.text}`}>
        <Icon icon={Play} size={12} /><span>{item.text}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  /* The rendered card (v1.2): calm reading typography on the app's surfaces. Hosts size it
     (the study card sets 24–40 px and shrinks long text); the note type's own CSS is scoped
     to `.card.<scope>` and wins over these defaults. */
  .card {
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    padding: var(--space-4);
    min-height: 4rem;
    color: var(--text-1);
    font-size: var(--text-lg);
    line-height: var(--leading);
    overflow-wrap: anywhere;
  }
  .card :global(p) {
    margin: 0 0 0.5em;
  }
  .card :global(p:last-child) {
    margin-bottom: 0;
  }
  .card :global(hr) {
    height: 0;
    margin: 0.75em 0;
    border: none;
    border-top: 1px solid var(--border-strong);
  }
  .card :global(ul),
  .card :global(ol) {
    margin: 0.25em 0;
    padding-inline-start: 1.25em;
    text-align: start;
  }
  .card :global(img),
  .card :global(video) {
    max-width: 100%;
    border-radius: 4px;
  }
  .card :global(a) {
    color: var(--accent-text);
  }
  .card :global(.cloze) {
    font-weight: 600;
    color: var(--accent-text);
  }
  /* Labelled answer parts of the built-in vocabulary backs (v1.3, fc_core `PARTS_CSS`): the
     meaning largest, the example set apart by a rule (sentence, then its translation), the
     source smallest; labels are filled by `enhance` (cards.part.*). Colours are tokens so both
     themes read; muting is opacity on full-contrast text, the same values the built-in CSS
     uses, so the two never compound. `:where` keeps these below the note type's own CSS and
     the host's (the study card sizes them). */
  .card :global(:where(.fc-part)) {
    margin-top: 0.75em;
  }
  .card :global(:where(.fc-part-meaning, .fc-part-answer)) {
    font-size: 1.3em;
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
    color: var(--text-1);
  }
  .card :global(:where(.fc-part-label)) {
    display: block;
    margin-bottom: 0.15em;
    font-size: 0.7em;
    font-weight: var(--weight-semibold);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-1);
    opacity: 0.65;
  }
  .card :global(:where(.fc-part-label:empty)) {
    display: none;
  }
  .card :global(:where(.fc-part-example)) {
    padding-left: 0.75em;
    border-left: 2px solid var(--border-strong);
    text-align: start;
  }
  .card :global(:where(.fc-example-translation)) {
    margin-top: 0.25em;
    color: var(--text-1);
    opacity: 0.7;
  }
  .card :global(:where(.fc-part-source)) {
    font-size: 0.8em;
    color: var(--text-1);
    opacity: 0.7;
  }
  .card :global(:where(.fc-part-source .fc-part-label)) {
    display: inline;
    margin-right: 0.4em;
  }
  /* The source is already muted: its label must not fade twice (0.7 × 0.65 is unreadable in
     the light theme). Deliberately above the note type's CSS. */
  .card :global(.fc-part-source .fc-part-label) {
    opacity: 1;
  }
  /* v1.3 X1 Subtitle front: the dictionary form small beside the form as it appeared (empty
     when they are the same; a space separates them in the text). */
  .card :global(:where(.fc-lemma)) {
    margin-inline-start: 0.25em;
    font-size: 0.5em;
    font-weight: var(--weight-regular);
    color: var(--text-1);
    opacity: 0.65;
  }
  .card :global(:where(.fc-lemma:empty)) {
    display: none;
  }
  /* A context hint's blank (`span.fc-blank`), wherever hint HTML is shown in a card. */
  .card :global(:where(.fc-blank)) {
    display: inline-block;
    min-width: 3em;
    border-bottom: 2px solid var(--accent-text);
    color: transparent;
  }
  .card :global(.fc-link-missing) {
    text-decoration: underline dotted;
    color: var(--text-2);
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
    background: var(--accent);
    opacity: 0.9;
    border-radius: 2px;
  }
  .card :global(.fc-io-active) {
    background: var(--danger-solid);
  }
  .card :global(.fc-io-revealed) {
    background: transparent;
    outline: 2px solid var(--danger-solid);
  }
  .card :global(code) {
    font-family: var(--font-mono);
    font-size: 0.85em;
    padding: 0.1em 0.3em;
    border-radius: 4px;
    background: var(--surface-2);
  }
  .card :global(pre) {
    background: var(--surface-2);
    border: 1px solid var(--border);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    overflow-x: auto;
    font-size: var(--text-sm);
    text-align: start;
  }
  .card :global(pre code) {
    padding: 0;
    background: transparent;
  }
  .card :global(.fc-hint summary) {
    cursor: pointer;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }
  .choices button {
    min-height: var(--control-h-lg);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-md);
    text-align: start;
    cursor: pointer;
  }
  .choices button:hover {
    background: var(--surface-3);
  }
  .choices button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .choices .right {
    border-color: var(--success);
    box-shadow: inset 2px 0 0 var(--success);
  }
  .choices .wrong {
    border-color: var(--danger);
    box-shadow: inset 2px 0 0 var(--danger);
  }
  .type-box {
    box-sizing: border-box;
    margin-top: var(--space-3);
    width: 100%;
    height: var(--control-h-lg);
    padding: 0 var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-md);
  }
  .type-box:focus-visible {
    outline: none;
    border-color: var(--accent-text);
    box-shadow: 0 0 0 1px var(--accent-text);
  }
  .type-box::placeholder {
    color: var(--text-3);
  }
  .expected {
    margin: var(--space-3) 0 0;
    font-size: var(--text-md);
  }
  .muted {
    color: var(--text-2);
  }
  .tts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-top: var(--space-2);
  }
  .speak {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: var(--control-h-sm);
    padding: 0 var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-xs);
    cursor: pointer;
  }
  .speak:hover {
    color: var(--text-1);
    background: var(--surface-3);
  }
  .speak:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
</style>
