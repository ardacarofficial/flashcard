<!--
  Filter chips under the Search field (v1.4 E3): each chip writes one predicate of the query
  language into the query text — visible, so the user learns the language — and × deletes it.
  Chips of a switched-off module are hidden. Token editing: `filter-chips.ts`; parsing and
  running the query stay in Rust.
-->
<script lang="ts">
  import { i18n, t } from '$lib/i18n/index.svelte';
  import { Check, Chip, ChevronDown, Icon, IconButton, Menu, X, type MenuItem } from '$lib/ui';
  import { moduleKnownOn } from '../cards/modules.svelte';
  import { chipDefs, findToken, removeToken, setToken, valueOf, type ChipDef, type ChipValue } from './filter-chips';

  let {
    query,
    onset,
    ontext,
  }: {
    query: string;
    /** A chip changed the query: run it. */
    onset: (query: string) => void;
    /** A text chip wrote `key:`: put the caret after it so the user types the value. */
    ontext: (query: string, caret: number) => void;
  } = $props();

  const defs = $derived(chipDefs(i18n.locale).filter((d) => !d.module || moduleKnownOn(d.module)));

  const valueLabel = (v: ChipValue) => (typeof v.label === 'string' ? t(v.label) : v.label.raw);

  /** The chip's token in the query: its label and the menu value it is (if one). */
  function current(def: ChipDef): { text: string; value: string | null } | null {
    const tok = findToken(query, def.keys);
    if (!tok) return null;
    const v = valueOf(def, tok);
    return { text: v ? valueLabel(v) : tok.value || tok.text, value: v?.token ?? null };
  }

  function items(def: ChipDef, active: string | null): MenuItem[] {
    return def.values.map((v) => ({
      id: v.token,
      label: valueLabel(v),
      icon: active === v.token ? Check : undefined,
      onselect: () => onset(setToken(query, def.keys, v.token)),
    }));
  }

  function writeKey(def: ChipDef) {
    const tok = findToken(query, def.keys);
    if (tok) {
      ontext(query, tok.end);
      return;
    }
    const q = setToken(query, def.keys, def.text!);
    ontext(q, q.length);
  }
</script>

<div class="chips" role="group" aria-label={t('browse.chips.label')} title={t('browse.chips.hint')} data-testid="filter-chips">
  {#each defs as def (def.id)}
    {@const cur = current(def)}
    {@const name = t(def.label)}
    <span class="chip" class:active={cur !== null} data-chip={def.id}>
      {#if def.text}
        <Chip
          size="sm"
          selected={cur !== null}
          label={cur ? t('browse.chips.active', { name, value: cur.text }) : name}
          onclick={() => writeKey(def)}
        />
      {:else}
        <Menu
          items={items(def, cur?.value ?? null)}
          label={cur ? t('browse.chips.active', { name, value: cur.text }) : name}
          align="start"
          triggerClass="chip-trigger"
        >
          {#snippet trigger()}
            <Chip size="sm" selected={cur !== null}>
              {cur ? t('browse.chips.active', { name, value: cur.text }) : name}<Icon icon={ChevronDown} size={12} />
            </Chip>
          {/snippet}
        </Menu>
      {/if}
      {#if cur}
        <IconButton size="sm" icon={X} label={t('browse.chips.remove', { name })} class="chip-x" onclick={() => onset(removeToken(query, def.keys))} />
      {/if}
    </span>
  {/each}
</div>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
  }
  .chip {
    display: inline-flex;
    align-items: center;
  }
  .chip :global(button.chip-trigger) {
    padding: 0;
    border-radius: var(--radius-pill);
  }
  .chip :global(button.chip-trigger:hover) {
    background: transparent;
  }
  .chip :global(button.chip-trigger:hover .fc-chip) {
    border-color: var(--border-strong);
  }
  .chip :global(.chip-x) {
    width: 22px;
    height: 22px;
    margin-inline-start: 2px;
  }
</style>
