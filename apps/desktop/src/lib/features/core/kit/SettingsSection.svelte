<!--
  One titled group of settings: a Card with a header (title, optional description and
  actions) and a body. Put `FormRow`s directly inside for divided rows, or free content.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconNode } from 'lucide';
  import { Card, Icon } from '$lib/ui';

  interface Props {
    title?: string;
    description?: string;
    icon?: IconNode;
    actions?: Snippet;
    /** Body padding: `rows` (FormRows supply their own) or `md` (free content). */
    body?: 'rows' | 'md';
    /** Heading level (2 inside a page, 3 nested). */
    level?: 2 | 3;
    id?: string;
    children: Snippet;
    class?: string;
  }

  let { title, description, icon, actions, body = 'md', level = 2, id, children, class: klass = '' }: Props = $props();
  const uid = $props.id();
  const headingId = $derived(id ?? `fc-section-${uid}`);
</script>

<section class="fc-settings-section {klass}" aria-labelledby={title ? headingId : undefined}>
  <Card padding="none">
    {#if title || actions}
      <header>
        <div class="titles">
          {#if title}
            <svelte:element this={`h${level}`} id={headingId} class="title">
              {#if icon}<Icon {icon} size={20} />{/if}<span>{title}</span>
            </svelte:element>
          {/if}
          {#if description}<p class="desc">{description}</p>{/if}
        </div>
        {#if actions}<div class="actions">{@render actions()}</div>{/if}
      </header>
    {/if}
    <div class="body {body}" class:headed={!!(title || actions)}>{@render children()}</div>
  </Card>
</section>

<style>
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
    flex-wrap: wrap;
    padding: var(--space-6) var(--space-6) 0;
  }
  .titles {
    min-width: 0;
    flex: 1 1 16rem;
  }
  .title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
    color: var(--text-1);
  }
  .title :global(svg) {
    color: var(--text-2);
    flex: none;
  }
  .desc {
    margin: var(--space-1) 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
    line-height: var(--leading);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .body.md {
    padding: var(--space-4) var(--space-6) var(--space-6);
  }
  .body.md:not(.headed) {
    padding-top: var(--space-6);
  }
  .body.rows {
    padding: var(--space-2) 0;
  }
  .body.rows.headed {
    padding-top: var(--space-3);
  }
  @media (max-width: 1023.98px) {
    header {
      padding-inline: var(--space-4);
    }
    .body.md {
      padding-inline: var(--space-4);
    }
  }
</style>
