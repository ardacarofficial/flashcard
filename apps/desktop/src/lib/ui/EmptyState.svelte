<!-- Empty list: one sentence and at most one action ("No decks yet — Create your first deck"). -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconNode } from 'lucide';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    title: string;
    description?: string;
    icon?: IconNode;
    action?: { label: string; onclick: () => void; icon?: IconNode };
    /** Extra content under the text (rare). */
    children?: Snippet;
    size?: 'sm' | 'md';
    class?: string;
  }

  let { title, description, icon, action, children, size = 'md', class: klass = '' }: Props = $props();
</script>

<div class="fc-empty {size} {klass}">
  {#if icon}<span class="icon"><Icon {icon} size={size === 'sm' ? 20 : 28} /></span>{/if}
  <p class="title">{title}</p>
  {#if description}<p class="desc">{description}</p>{/if}
  {#if children}{@render children()}{/if}
  {#if action}
    <Button variant="primary" icon={action.icon} onclick={action.onclick}>{action.label}</Button>
  {/if}
</div>

<style>
  .fc-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-12) var(--space-6);
    text-align: center;
    color: var(--text-1);
  }
  .sm {
    padding: var(--space-6) var(--space-4);
  }
  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    margin-bottom: var(--space-2);
    border-radius: 50%;
    background: var(--surface-2);
    color: var(--text-2);
  }
  .sm .icon {
    width: 40px;
    height: 40px;
  }
  .title {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .desc {
    margin: 0 0 var(--space-2);
    max-width: 44ch;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
</style>
