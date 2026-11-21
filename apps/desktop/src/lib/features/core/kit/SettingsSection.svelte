<!--
  One titled group of a Settings (or Statistics) page (v1.2): a small heading with an optional
  one-line description and actions, then the body — no box around it. `body="rows"` stacks
  `FormRow`s divided by 1px lines (the line above the first row closes the heading); `md` is
  free content. `tone="danger"` marks the destructive group, which goes last on a page and
  is set apart by extra space and a red heading.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconNode } from 'lucide';
  import { Icon } from '$lib/ui';

  interface Props {
    title?: string;
    description?: string;
    /** Small leading icon of the heading (kept quiet: tertiary colour). */
    icon?: IconNode;
    actions?: Snippet;
    /** `rows` (divided FormRows) or `md` (free content). */
    body?: 'rows' | 'md';
    /** Heading level (2 inside a page, 3 nested). */
    level?: 2 | 3;
    /** `danger`: the destructive group (last on the page). */
    tone?: 'default' | 'danger';
    id?: string;
    children: Snippet;
    class?: string;
  }

  let { title, description, icon, actions, body = 'md', level = 2, tone = 'default', id, children, class: klass = '' }: Props = $props();
  const uid = $props.id();
  const headingId = $derived(id ?? `fc-section-${uid}`);
</script>

<section class="fc-settings-section {klass}" class:danger={tone === 'danger'} aria-labelledby={title ? headingId : undefined}>
  {#if title || actions || description}
    <header>
      <div class="titles">
        {#if title}
          <svelte:element this={`h${level}`} id={headingId} class="title">
            {#if icon}<Icon {icon} size={14} />{/if}<span>{title}</span>
          </svelte:element>
        {/if}
        {#if description}<p class="desc">{description}</p>{/if}
      </div>
      {#if actions}<div class="actions">{@render actions()}</div>{/if}
    </header>
  {/if}
  <div class="body {body}">{@render children()}</div>
</section>

<style>
  .fc-settings-section {
    min-width: 0;
  }
  .fc-settings-section.danger {
    margin-top: var(--space-4);
  }
  header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    flex-wrap: wrap;
    padding-bottom: var(--space-2);
  }
  .titles {
    min-width: 0;
    flex: 1 1 16rem;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
    color: var(--text-1);
  }
  .title :global(svg) {
    color: var(--text-3);
    flex: none;
  }
  .danger .title {
    color: var(--danger);
  }
  .danger .title :global(svg) {
    color: var(--danger);
  }
  .desc {
    margin: 2px 0 0;
    color: var(--text-2);
    font-size: var(--text-xs);
    line-height: var(--leading);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .body.rows {
    border-top: 1px solid var(--border);
  }
  .danger .body.rows {
    border-top-color: color-mix(in srgb, var(--danger) 45%, var(--border));
  }
  .body.md {
    padding-top: var(--space-1);
  }
</style>
