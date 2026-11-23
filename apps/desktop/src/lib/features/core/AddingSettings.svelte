<!--
  Settings › Advanced › App › Adding cards (v1.4): the quick-add shortcut and clipboard
  prefill, clipboard suggestions and remembered classifications — settings that had no page
  in v1.3 (only commands). Rows come from `settings.schema`; a switched-off module's rows
  are not there.
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { ClipboardPaste, Plus, Tag } from '$lib/ui/icons';
  import { SchemaRows, SettingsPage, SettingsSection } from './kit';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();
</script>

<SettingsPage title={t('settings_pages.adding.title')} subtitle={t('settings_pages.adding.intro')}>
  <SettingsSection title={t('settings_pages.adding.quick')} icon={Plus} body="rows">
    <SchemaRows keys={['quick_add.shortcut', 'quick_add.prefill_clipboard']} guarded={ctx.guarded} />
  </SettingsSection>
  <SettingsSection title={t('settings_pages.adding.clipboard')} icon={ClipboardPaste} body="rows">
    <SchemaRows keys={['clipboard.min_chars', 'clipboard.max_chars']} guarded={ctx.guarded} />
  </SettingsSection>
  <SettingsSection title={t('settings_pages.adding.attributes')} icon={Tag} body="rows">
    <SchemaRows keys={['attributes.remember_last']} guarded={ctx.guarded} />
  </SettingsSection>
</SettingsPage>
