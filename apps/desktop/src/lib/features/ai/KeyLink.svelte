<!--
  A page the user opens in their browser (where to get a key, where to download a local
  server): the address as a link and as text, with a copy button — the desktop shell may not
  open links itself.
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { Copy, ExternalLink, Icon, IconButton } from '$lib/ui';

  let { url, label }: { url: string; label: string } = $props();
  let copied = $state(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(url);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      copied = false;
    }
  }
</script>

<p class="link">
  <span>{label}</span>
  <a href={url} target="_blank" rel="noopener noreferrer">{url}<Icon icon={ExternalLink} size={12} /></a>
  <IconButton icon={Copy} size="sm" label={copied ? t('ai.setup.copied') : t('ai.setup.copy_link')} onclick={() => void copy()} />
</p>

<style>
  .link {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  a {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--accent-text);
    overflow-wrap: anywhere;
  }
</style>
