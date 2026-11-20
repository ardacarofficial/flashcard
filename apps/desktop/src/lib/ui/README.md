# `$lib/ui` — design system (docs/11 §4)

Every screen builds its UI from these components and the tokens in `src/lib/theme/tokens.css`.
Import from the barrel: `import { Button, Select, Plus } from '$lib/ui';`

Rules for screen lanes:

- **No raw controls**: no `<select>`, `<input type="checkbox|radio">` outside this folder
  (lint: `tests/raw-controls.test.ts`). When you restyle a screen, remove its entry from
  `tests/raw-controls.allow.json` (the test fails until you do; the list only shrinks).
- **No raw colours / sizes**: use tokens (`--surface-0/1/2`, `--text-1/2/3`, `--accent`,
  `--accent-fg`, `--accent-soft`, `--success|warning|danger` (+ `-solid`, `-solid-fg`, `-soft`),
  `--border`, `--border-strong`, `--space-1…16`, `--radius-control|card|lg|pill`,
  `--shadow-1|2`, `--text-xs…3xl`, `--motion-fast|motion|motion-slow`). The `--fc-*`
  variables are the backend theme layer — don't use them in new code.
- **All text is i18n**: components take translated strings (`label={t('…')}`); their own
  built-in texts are under `ui.*` (`locales/*/ui.json`).
- **Icons**: only via `icons.ts` (Lucide subset). Need a new one? Add it to `icons.ts`.
  `iconFor(key)` maps backend `UiContribution.icon` keys.
- **Navigation**: screens link with `ctx.navigate('/route')` (never import other screens).
- Colour is never the only cue: pair it with an icon or text.

Common props: most components accept `class`; sizes are `'sm' | 'md' | 'lg'`; tones are
`'neutral' | 'accent' | 'success' | 'warning' | 'danger'`. Bindable props are marked `bind:`.

## Actions

| Component | Props | Usage |
|---|---|---|
| **Button** | `variant` primary/secondary(default)/ghost/danger · `size` · `loading` · `icon` · `iconEnd` · `fullWidth` · `disabled` · `type` · `bind:element` · any `<button>` attr · children | `<Button variant="primary" icon={Plus} onclick={add}>{t('deck.new')}</Button>` |
| **IconButton** | `icon` · `label` (required: accessible name + hover hint) · `variant` ghost(default)/secondary/primary/danger · `size` · `pressed` (toggle) · `loading` | `<IconButton icon={Pencil} label={t('common.rename')} onclick={edit} />` |
| **Menu** | `items: MenuItem[]` (`{id,label,icon?,keys?,danger?,disabled?,onselect}` or `{id,separator:true}`) · `label` (trigger name, default "More actions") · `icon` · `trigger` snippet (custom content) · `align` start/end · `placement` bottom/top | `<Menu items={[{ id: 'del', label: t('common.delete'), danger: true, onselect: del }]} />` |
| **Tile** | `icon` · `title` · `description?` · `meta?` · `disabled` · `onclick` | `<Tile icon={Repeat} title={t('…')} description={t('…')} onclick={learn} />` |

## Form controls

All wrap **Field** (label, hint, error with `aria-describedby`/`aria-invalid`). Shared: `label`,
`hint`, `error`, `hideLabel`, `required`, `disabled`, `id`, `size`.

| Component | Props | Usage |
|---|---|---|
| **Input** | `bind:value` · `type` text/search/email/password/number/url/tel/date/time/color · `icon` (leading) · `placeholder` · `bind:element` · any `<input>` attr | `<Input label={t('deck.name')} bind:value={name} />` |
| **Textarea** | `bind:value` · `rows` · `autoGrow` · `maxRows` | `<Textarea label={t('…')} bind:value={text} autoGrow />` |
| **Select** | `bind:value` · `options: SelectOption<T>[]` (`{value,label,description?,disabled?}`) · `placeholder` · `onchange(value)` — generic, custom list box: ↑/↓ Home/End Enter/Space Esc, typeahead | `<Select label={t('deck.current')} options={opts} bind:value={deck} />` |
| **Checkbox** | `bind:checked` · `indeterminate` · `label` / children / `ariaLabel` · `description` · `onchange(checked)` | `<Checkbox label={t('…')} bind:checked={on} />` |
| **Switch** | `bind:checked` · `label` / `ariaLabel` · `description` · `onchange(checked)` — for settings that apply at once | `<Switch label={t('…')} bind:checked={enabled} />` |
| **SegmentedControl** | `bind:value` · `options` (with optional `icon`) · `label` (group name) · `size` sm/md · `fullWidth` · `onchange` — 2–5 short choices, arrow keys | `<SegmentedControl label={t('…')} options={modes} bind:value={mode} />` |
| **RadioGroup** | `bind:value` · `options` (with `description`) · `label` (legend) · `hideLabel` · `name` · `onchange` — vertical list | `<RadioGroup label={t('…')} options={opts} bind:value={v} />` |
| **Field** | `label` · `hint` · `error` · `id` · `control` snippet `({ id, describedBy, invalid })` — for custom controls | `<Field label={…}>{#snippet control(f)}<MyInput id={f.id} />{/snippet}</Field>` |

## Display

| Component | Props | Usage |
|---|---|---|
| **Card** | `padding` none/sm/md(default)/lg · `elevated` · `onclick` (whole card becomes a button) · `label` · `as` div/section/article/li | `<Card padding="lg">…</Card>` |
| **Chip** | `label` / children · `tone` · `icon` · `size` sm/md · `selected` + `onclick` (toggle, `aria-pressed`) · `onremove` (× button) | `<Chip tone="accent" label="B1" />` |
| **Badge** | `count` (hidden when 0/null; `max` → "99+") · `tone` · `variant` solid/soft · `label` (accessible text) · children | `<Badge count={n} label={t('…', { count: n })} />` |
| **Avatar** | `name` (initials) · `src?` · `size` · `label?` (decorative without) | `<Avatar name={profile} />` |
| **Kbd** | `keys: string[] \| 'Mod+K'` — `Mod` = ⌘ on macOS, Ctrl elsewhere | `<Kbd keys={['Mod', 'K']} />` |
| **Icon** | `icon` (from `icons.ts`) · `size` (16/20/24) · `strokeWidth` · `label?` (decorative without) | `<Icon icon={Search} size={20} />` |
| **ProgressBar** | `label` (required) · `value` · `max` · or `segments: {value,label,tone}[]` (stacked, labels in the accessible name) · `tone` · `size` · `showValue` | `<ProgressBar label={t('…')} segments={[{ value: 3, label: t('…'), tone: 'success' }]} />` |
| **ProgressRing** | `label` · `value` · `max` · `size` px · `stroke` · `tone` · children (centre content, default %) | `<ProgressRing label={t('…')} value={score} />` |
| **EmptyState** | `title` · `description?` · `icon?` · `action?: {label,onclick,icon?}` (one action) · `size` sm/md | `<EmptyState title={t('…')} action={{ label: t('…'), onclick: create }} />` |
| **Skeleton** | `width` · `height` · `shape` text/control/card/circle · `lines` | `<Skeleton shape="card" />` |
| **Spinner** | `size` (decorative; announce loading in text) | `<Spinner />` |
| **Table** | `rows` · `columns: TableColumn<T>[]` (`{id,header,width?,align?,value?}`) · `rowKey` · `label` · `rowHeight` (fixed, 44) · `height` (CSS) · `bind:activeKey` · `selectedKeys` · `onrowclick` · `cell` snippet `(row, column)` · `empty` snippet — virtualised grid; ↑/↓ PgUp/PgDn Home/End, Enter | `<Table rows={cards} {columns} rowKey={(c) => c.id} label={t('…')} onrowclick={open} />` |
| **Flashcard** | `front` / `back` snippets · `bind:flipped` · `onflip(flipped)` · `disabled` · `instant` · `fit` (shrink long text, default on) · `size` md/lg — Space/Enter/click flip; 3D flip off under `prefers-reduced-motion` | `<Flashcard bind:flipped>{#snippet front()}…{/snippet}{#snippet back()}…{/snippet}</Flashcard>` |

## Overlays and feedback

| Component | Props | Usage |
|---|---|---|
| **Dialog** | `bind:open` · `title` · `description?` · `variant` dialog/sheet · `size` · `dismissible` (Esc, backdrop, ×; default true) · `role` dialog/alertdialog · `initialFocus` (selector) · `onclose` · children · `footer` snippet — focus trap, focus return | `<Dialog bind:open title={t('…')}>…{#snippet footer()}<Button …/>{/snippet}</Dialog>` |
| **Tooltip** | `text` · `placement` top/bottom/start/end · `delay` · children (the anchor) — hints only | `<Tooltip text={t('…')}><IconButton …/></Tooltip>` |
| **toast** / **ToastHost** / **Toast** | `toast.show(message, { tone?, action?: {label, run}, duration? })` → id · `toast.dismiss(id)`. The shell mounts `ToastHost` | `toast.show(t('…'), { tone: 'success' })` |

## Navigation and layout

| Component | Props | Usage |
|---|---|---|
| **PageHeader** | `title` (the page's `<h1>`) · `subtitle?` · `back?: {label,onclick}` · `meta` snippet (chips) · `actions` snippet · `tabs` snippet | `<PageHeader title={deck.name}>{#snippet actions()}<Menu …/>{/snippet}</PageHeader>` |
| **Tabs** | `tabs: TabItem[]` (`{id,label,icon?,badge?,disabled?}`) · `bind:value` · `label` · `variant` line/pill · `onchange(id)` · children snippet `(id)` (panel) | `<Tabs label={t('…')} {tabs} bind:value={tab} />` |
| **Sidebar** | `label` · `collapsed` · `header` / children (NavItems) / `footer` snippets — icon-only below 1024 px | (app shell) |
| **NavItem** | `label` · `icon` · `active` · `accent` · `badge` · `badgeLabel` · `collapsed` · `onclick` · `leading` snippet | (app shell) |

## Shell (`src/lib/shell/`)

- `navigation.ts` — the only place mapping module routes to the 5 sections, the Inbox or a
  Settings group (`PLACEMENTS`). A new module screen: add its route there. Order within a
  section/group = order in that list.
- Sections with more than one page show them as tabs above the screen; Settings shows a grouped
  sub-navigation. The Home page (`HomeScreen.svelte`) is a placeholder for the Home lane.
