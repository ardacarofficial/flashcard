<script lang="ts">
  import type { UiNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { uiText } from './texts.svelte';
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
  <label>
    {text(node.label)}
    {#if node.multiline}
      <textarea value={String(values[node.id] ?? node.value ?? '')} oninput={(e) => (values[node.id] = e.currentTarget.value)}></textarea>
    {:else}
      <input
        value={String(values[node.id] ?? node.value ?? '')}
        oninput={(e) => (values[node.id] = e.currentTarget.value)}
        onkeydown={(e) => {
          if (e.key === 'Enter') onaction('submit');
        }}
      />
    {/if}
  </label>
{:else if node.kind === 'choice'}
  <fieldset>
    {#if node.label}<legend>{text(node.label)}</legend>{/if}
    {#each node.options as o (o.value)}
      <label class="option">
        <input type="radio" name={node.id} value={o.value} checked={values[node.id] === o.value} onchange={() => (values[node.id] = o.value)} />
        {text(o.label)}
      </label>
    {/each}
  </fieldset>
{:else if node.kind === 'audio_recorder'}
  <p class="muted">{t('plugins.view.recorder_unavailable')}</p>
{:else if node.kind === 'audio'}
  <audio controls src={node.src.replace(/^media:/, 'fcmedia://')}></audio>
{:else if node.kind === 'button'}
  <button class:primary={node.primary} disabled={busy} onclick={() => onaction(node.action)}>{text(node.label)}</button>
{:else if node.kind === 'feedback'}
  <p class="feedback" class:correct={node.correct} class:wrong={!node.correct} role="status">{text(node.text)}</p>
{:else if node.kind === 'progress'}
  <progress value={node.value} max={Math.max(node.max, 1)}></progress>
{/if}

<style>
  .column {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
  }
  label {
    display: block;
  }
  label input:not([type='radio']),
  label textarea {
    display: block;
    width: 100%;
    max-width: 28rem;
  }
  .option {
    display: flex;
    gap: var(--fc-space-1);
    align-items: center;
  }
  .feedback.correct {
    color: var(--fc-success, green);
  }
  .feedback.wrong {
    color: var(--fc-danger, firebrick);
  }
</style>
