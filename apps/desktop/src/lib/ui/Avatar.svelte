<!-- Avatar: initials of `name` (or an image). Decorative unless `label` is given. -->
<script lang="ts">
  interface Props {
    name: string;
    src?: string | null;
    size?: 'sm' | 'md' | 'lg';
    label?: string;
    class?: string;
  }

  let { name, src, size = 'md', label, class: klass = '' }: Props = $props();

  const initials = $derived(
    name
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w.charAt(0).toLocaleUpperCase())
      .join(''),
  );
</script>

<span class="fc-avatar {size} {klass}" role={label ? 'img' : undefined} aria-label={label} aria-hidden={label ? undefined : 'true'}>
  {#if src}<img {src} alt="" />{:else}{initials}{/if}
</span>

<style>
  .fc-avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    overflow: hidden;
    border-radius: 50%;
    background: var(--surface-3);
    border: 1px solid var(--border-strong);
    color: var(--text-1);
    font-weight: var(--weight-semibold);
    line-height: 1;
    user-select: none;
  }
  .sm {
    width: 24px;
    height: 24px;
    font-size: 10px;
  }
  .md {
    width: 28px;
    height: 28px;
    font-size: var(--text-2xs);
  }
  .lg {
    width: 40px;
    height: 40px;
    font-size: var(--text-md);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
