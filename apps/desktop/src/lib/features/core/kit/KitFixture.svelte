<!-- Test fixture (kit.test.ts): a page with an everyday row and an "Advanced" disclosure. The
     texts come from the test (`texts`), so the component itself holds none. -->
<script lang="ts">
  import FormRow from './FormRow.svelte';
  import SettingsAdvanced from './SettingsAdvanced.svelte';
  import SettingsPage from './SettingsPage.svelte';
  import SettingsSection from './SettingsSection.svelte';

  interface Props {
    texts: { page: string; row: string; desc: string; example: string; rare: string; rareDesc: string };
    isDefault?: boolean;
    onReset?: () => void;
  }
  let { texts, isDefault, onReset }: Props = $props();
</script>

<SettingsPage title={texts.page}>
  <SettingsSection body="rows">
    <FormRow label={texts.row} description={texts.desc} example={texts.example} {isDefault} {onReset} />
  </SettingsSection>
  <SettingsAdvanced id="fixture">
    <SettingsSection body="rows">
      <FormRow label={texts.rare} description={texts.rareDesc} />
    </SettingsSection>
  </SettingsAdvanced>
</SettingsPage>
