<!--
  "Advanced: template code" of a note type (v1.3): the technical side of the definition,
  everything the editor could change before — each field's kind / unique / searchable, the
  list field, the card templates (name, card kind, front and back HTML) and the CSS.
  Edits go to the same draft as the everyday view; the page saves it.
-->
<script lang="ts">
  import type { FieldKind, NoteTypeInput } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Checkbox, IconButton, Input, Plus, Select, Textarea, Trash2 } from '$lib/ui';
  import { FormRow, SettingsSection } from '../../core/kit';

  interface Props {
    draft: NoteTypeInput;
    engineOptions: { value: string; label: string }[];
    /** Live notes of the type (new templates make cards for them). */
    notes: number;
    onremovetemplate: (i: number) => void;
  }
  let { draft = $bindable(), engineOptions, notes, onremovetemplate }: Props = $props();

  const KINDS: FieldKind[] = ['rich_text', 'text', 'image', 'audio', 'video', 'number', 'tag_list', 'link'];
  const kindOptions = $derived(KINDS.map((k) => ({ value: k, label: t(`notetype.kind.${k}`) })));
  const sortOptions = $derived(draft.fields.filter((f) => f.key).map((f) => ({ value: f.key, label: f.name })));
  const codes = $derived(draft.fields.filter((f) => f.key));

  function addTemplate() {
    draft.templates = [...draft.templates, { key: '', name: t('notetype.new_template'), front: '', back: '{{FrontSide}}\n<hr>\n', card_type: 'classic' }];
  }
</script>

<div class="nt-code">
<SettingsSection title={t('notetype.code.fields_title')} description={t('notetype.code.fields_desc')} level={3}>
  <div class="ftable" role="group" aria-label={t('notetype.code.fields_title')}>
    <div class="frow fhead" aria-hidden="true">
      <span>{t('notetype.field_name')}</span>
      <span>{t('notetype.field_kind')}</span>
      <span class="c">{t('notetype.unique')}</span>
      <span class="c">{t('notetype.searchable')}</span>
    </div>
    {#each draft.fields as f, i (i)}
      <div class="frow">
        <span class="fname">{f.name || t('notetype.field_n', { n: i + 1 })}{#if f.key}<code>{f.key}</code>{/if}</span>
        <Select size="sm" options={kindOptions} bind:value={f.kind} label={`${t('notetype.field_kind')}: ${f.name}`} hideLabel />
        <span class="c"><Checkbox bind:checked={f.unique} ariaLabel={`${t('notetype.unique')}: ${f.name}`} /></span>
        <span class="c"><Checkbox bind:checked={f.searchable} ariaLabel={`${t('notetype.searchable')}: ${f.name}`} /></span>
      </div>
    {/each}
  </div>
  <ul class="legend">
    <li><strong>{t('notetype.unique')}:</strong> {t('notetype.code.unique_hint')}</li>
    <li><strong>{t('notetype.searchable')}:</strong> {t('notetype.code.searchable_hint')}</li>
  </ul>
</SettingsSection>

<SettingsSection level={3} body="rows">
  <FormRow label={t('notetype.sort_field')} description={t('notetype.sort_field_hint')}>
    <Select options={sortOptions} bind:value={draft.sort_field} label={t('notetype.sort_field')} hideLabel />
  </FormRow>
</SettingsSection>

<SettingsSection title={t('notetype.templates')} description={t('notetype.syntax_intro')} level={3}>
  {#snippet actions()}
    {#if draft.kind === 'standard'}
      <Button size="sm" icon={Plus} onclick={addTemplate}>{t('notetype.add_template')}</Button>
    {/if}
  {/snippet}
  <dl class="help">
    <dt>{t('notetype.code.codes_label')}</dt>
    <dd>
      {#each codes as f (f.key)}<span class="code-chip"><code>{`{{${f.key}}}`}</code> {f.name}</span>{/each}
      {#if codes.length < draft.fields.length}<span class="muted">{t('notetype.code.codes_new')}</span>{/if}
    </dd>
    <dt>{t('notetype.code.syntax_label')}</dt>
    <dd><code>{t('notetype.syntax')}</code></dd>
  </dl>
  <div class="templates">
    {#each draft.templates as tpl, i (i)}
      <div class="tpl" data-testid="template-block">
        <div class="tpl-head">
          <Input size="sm" bind:value={tpl.name} label={t('notetype.template_name')} />
          <Select size="sm" options={engineOptions} bind:value={tpl.card_type} label={t('notetype.card_type')} />
          {#if draft.templates.length > 1}
            <IconButton size="sm" icon={Trash2} label={t('notetype.remove_template', { name: tpl.name })} onclick={() => onremovetemplate(i)} />
          {/if}
        </div>
        {#if !tpl.key && notes > 0}<p class="note">{t('notetype.code.new_template_cards', { notes })}</p>{/if}
        <div class="sides">
          <Textarea class="code" rows={4} bind:value={tpl.front} spellcheck="false" label={t('notetype.code.front_html')} />
          <Textarea class="code" rows={4} bind:value={tpl.back} spellcheck="false" label={t('notetype.code.back_html')} />
        </div>
      </div>
    {/each}
  </div>
</SettingsSection>

<SettingsSection title={t('notetype.css')} description={t('notetype.code.css_desc')} level={3}>
  <Textarea class="code" rows={5} bind:value={draft.css} spellcheck="false" label={t('notetype.css')} hideLabel />
</SettingsSection>
</div>

<style>
  .ftable {
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    overflow: hidden;
  }
  .frow {
    display: grid;
    grid-template-columns: minmax(7rem, 1.3fr) minmax(8rem, 1fr) repeat(2, 5.5rem);
    gap: var(--space-2);
    align-items: center;
    min-height: var(--row-h);
    padding: 3px var(--space-3);
    border-top: 1px solid var(--border);
    font-size: var(--text-sm);
  }
  .frow :global(.fc-field) {
    min-width: 0;
  }
  .fhead {
    min-height: 28px;
    border-top: none;
    background: var(--surface-2);
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
  }
  .fname {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fname code,
  .code-chip code,
  .help code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-2);
    font-variant-ligatures: none;
  }
  .c {
    display: flex;
    justify-content: center;
  }
  .legend {
    margin: var(--space-3) 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--text-sm);
    color: var(--text-2);
    line-height: var(--leading);
  }
  .legend strong {
    color: var(--text-1);
    font-weight: var(--weight-medium);
  }
  .help {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: var(--space-2) var(--space-3);
    margin: 0 0 var(--space-4);
    padding: var(--space-3);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    font-size: var(--text-sm);
  }
  .help dt {
    color: var(--text-2);
  }
  .help dd {
    display: flex;
    flex-wrap: wrap;
    gap: 4px var(--space-3);
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .code-chip {
    white-space: nowrap;
  }
  .muted {
    color: var(--text-2);
  }
  .templates {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .tpl {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
  }
  .tpl-head {
    display: flex;
    gap: var(--space-3);
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .tpl-head :global(.fc-field) {
    flex: 1 1 12rem;
  }
  .note {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .sides {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
    gap: var(--space-3);
  }
  .nt-code {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
  }
  .nt-code :global(.code textarea),
  .nt-code :global(textarea.code) {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }
</style>
