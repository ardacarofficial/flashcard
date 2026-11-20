<script lang="ts">
  import type { UiNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { uiText } from './texts.svelte';
  import { Button, CircleAlert, CircleCheck, Icon, Input, ProgressBar, RadioGroup, Textarea } from '$lib/ui';
  import PluginNode from './PluginNode.svelte';

  /** Draws one node of a plugin's declarative UI with the app's own components. */
  interface Props {
    node: UiNode;
    pluginId: string;
    /** Input values by node id (shared, mutated in place). */
    values: Record<string, unknown>;
    onaction: (action: string) => void;
    busy: boolean;
  }
  let { node, pluginId, values, onaction, busy }: Props = $props();
  const text = (x: Parameters<typeof uiText>[1]) => uiText(pluginId, x);
</script>

{#if node.kind === 'column'}
  <div class="column">
    {#each node.children as child, i (i)}<PluginNode node={child} {pluginId} {values} {onaction} {busy} />{/each}
  </div>
{:else if node.kind === 'row'}
  <div class="row">
    {#each node.children as child, i (i)}<PluginNode node={child} {pluginId} {values} {onaction} {busy} />{/each}
  </div>
{:else if node.kind === 'heading'}
  <h3>{text(node.text)}</h3>
{:else if node.kind === 'text'}
  <p>{text(node.text)}</p>
{:else if node.kind === 'html'}
  <!-- Sanitized by the backend (fc_core::content::sanitize_html). -->
  <div class="html">{@html node.html}</div>
{:else if node.kind === 'input'}
  {#if node.multiline}
    <Textarea label={text(node.label)} autoGrow value={String(values[node.id] ?? node.value ?? '')}
      oninput={(e: Event) => (values[node.id] = (e.currentTarget as HTMLTextAreaElement).value)} />
  {:else}
    <Input
      label={text(node.label)}
      value={String(values[node.id] ?? node.value ?? '')}
      oninput={(e: Event) => (values[node.id] = (e.currentTarget as HTMLInputElement).value)}
      onkeydown={(e: KeyboardEvent) => {
        if (e.key === 'Enter') onaction('submit');
      }}
    />
  {/if}
{:else if node.kind === 'choice'}
  <RadioGroup
    label={node.label ? text(node.label) : t('plugins.view.choice')}
    hideLabel={!node.label}
    name={node.id}
    options={node.options.map((o) => ({ value: o.value, label: text(o.label) }))}
    value={values[node.id] as string | undefined}
    onchange={(v: string) => (values[node.id] = v)}
  />
{:else if node.kind === 'audio_recorder'}
  <p class="muted">{t('plugins.view.recorder_unavailable')}</p>
{:else if node.kind === 'audio'}
  <audio controls src={node.src.replace(/^media:/, 'fcmedia://')}></audio>
{:else if node.kind === 'button'}
  <div><Button variant={node.primary ? 'primary' : 'secondary'} disabled={busy} onclick={() => onaction(node.action)}>{text(node.label)}</Button></div>
{:else if node.kind === 'feedback'}
  <p class="feedback" class:correct={node.correct} class:wrong={!node.correct} role="status"><Icon icon={node.correct ? CircleCheck : CircleAlert} size={20} /><span>{text(node.text)}</span></p>
{:else if node.kind === 'progress'}
  <ProgressBar label={t('plugins.view.progress')} value={node.value} max={Math.max(node.max, 1)} />
{/if}

<style>
  .column {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  h3 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  p {
    margin: 0;
  }
  .muted {
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .feedback {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    font-weight: var(--weight-medium);
  }
  .feedback.correct {
    background: var(--success-soft);
    color: var(--success);
  }
  .feedback.wrong {
    background: var(--danger-soft);
    color: var(--danger);
  }
</style>
