# `$lib/ui` — design system (docs/11 §4, v1.2 brief `.orchestra/DESIGN-BRIEF.md`)

v1.2 look: dark-first, dense, 1px borders and surface steps instead of shadows, one amber
accent. Body 13 px (`--text-sm`), controls 28/32/36 (`--control-h-sm|md|lg`), rows 34
(`--row-h`), radius 6 controls / 8 panels / 12 only the study flashcard, focus = 2 px ring
with 2 px offset. Numbers, counts, durations and key hints use JetBrains Mono
(`--font-mono`, bundled; `<span class="num">` = mono + tabular figures).

Every screen builds its UI from these components and the tokens in `src/lib/theme/tokens.css`.
Lazy screens import from the barrel: `import { Button, Select, Plus } from '$lib/ui';`.
Code loaded before the first screen (`main.ts`, `App.svelte`, `lib/shell/`, the always-mounted
hosts) imports file by file — `import Button from '$lib/ui/Button.svelte'`,
`import { Plus } from '$lib/ui/icons'` — never the barrel (lint: `tests/eager-imports.test.ts`). — **in lazy screens**.
Eagerly loaded code (App.svelte, `src/lib/shell/`, the `*Host` components) imports single files
instead (`import Button from '$lib/ui/Button.svelte'`, icons from `$lib/ui/icons`): Rollup puts
every module the entry reaches statically into the initial chunk, so a barrel import there would
pull every component any lazy screen uses (and its CSS) into the initial bundle.

Rules for screen lanes:

- **No raw controls**: no `<select>`, `<input type="checkbox|radio">` outside this folder
  (lint: `tests/raw-controls.test.ts`, no allow-list — zero exceptions since v1.1).
- **No raw colours / sizes**: use tokens (`--surface-0/1/2/3` (app bg, panels, raised/hover,
  selected), `--text-1/2/3` (3 = placeholders and decorative icons only), `--border`,
  `--border-strong`; accent: `--accent` (fill) + `--accent-fg` (text on it), `--accent-text`
  (the accent as text or icon on a surface: AA, darker in light — never `color: var(--accent)`),
  `--accent-soft` (14% tint); `--success|warning|danger` (+ `-solid`, `-solid-fg`, `-soft`);
  `--space-1…16`, `--radius-control|card|lg|flashcard|pill`, `--shadow-float` (floating layers
  only; `--shadow-1` is a no-op), `--backdrop`, `--text-2xs…3xl`, `--font-mono`,
  `--control-h-*`, `--row-h`, `--motion-fast|motion|motion-slow`). The `--fc-*` variables are
  the backend theme layer — don't use them in new code.
- **All text is i18n**: components take translated strings (`label={t('…')}`); their own
  built-in texts are under `ui.*` (`locales/*/ui.json`).
- **Icons**: only via `icons.ts` (Lucide subset). Need a new one? Add it to `icons.ts`.
  `iconFor(key)` maps backend `UiContribution.icon` keys.
- **Navigation**: screens link with `ctx.navigate('/route')` (never import other screens).
- Colour is never the only cue: pair it with an icon or text.

**Numbers and dates**: only through `$lib/format` (`formatPercent`, `formatPercent100`,
`formatDecimal`, `formatCount`, `formatSeconds`, `formatDuration`, `formatDate`,
`formatDateTime`, `formatRelative`, …) — Intl for the active language (TR `%35`, `15,0 sn`;
EN `35%`, `15.0 s`). No `toFixed(` or hand-glued `%` in components (lint:
`tests/number-format.test.ts`); translations take the formatted string (`"{pct} ustalaşıldı"`).

**Page width**: the shell's `.page` is the one container (`--content-max`, `--content-pad-x`,
left-aligned next to the sidebar); a screen does not centre itself or set its own page width.
Tables and charts use the wide variant (`ScreenDef.width: 'wide'` → `--content-max-wide`).

Common props: most components accept `class`; sizes are `'sm' | 'md' | 'lg'`; tones are
`'neutral' | 'accent' | 'success' | 'warning' | 'danger'`. Bindable props are marked `bind:`.

## Actions

| Component | Props | Usage |
|---|---|---|
| **Button** | `variant` primary/secondary(default)/ghost/danger · `size` · `loading` · `icon` · `iconEnd` · `kbd` (key hint after the label, e.g. `"N"`, `"Mod+Enter"`; also set `aria-keyshortcuts`) · `fullWidth` (the label stays centred, a `kbd` hint sits at the end) · `disabled` (muted colours, label and hint stay legible) · `type` · `bind:element` · any `<button>` attr · children | `<Button variant="primary" icon={Plus} kbd="N" onclick={add}>{t('deck.new')}</Button>` |
| **IconButton** | `icon` · `label` (required: accessible name + hover hint) · `variant` ghost(default)/secondary/primary/danger · `size` · `pressed` (toggle) · `loading` | `<IconButton icon={Pencil} label={t('common.rename')} onclick={edit} />` |
| **Menu** | `items: MenuItem[]` (`{id,label,icon?,keys?,danger?,disabled?,onselect}` or `{id,separator:true}`) · `label` (trigger name, default "More actions") · `icon` · `trigger` snippet (custom content) · `align` start/end · `placement` bottom/top | `<Menu items={[{ id: 'del', label: t('common.delete'), danger: true, onselect: del }]} />` |
| **Tile** | `icon` · `title` · `description?` · `meta?` · `disabled` · `onclick` · `testid` (`data-testid`) | `<Tile icon={Repeat} title={t('…')} description={t('…')} onclick={learn} />` |
| **OptionTile** | `label` / children · `kbd` (shortcut key) or `letter` (A, B…) · `checked` (set → `role="radio"`, put tiles in a `role="radiogroup"`) · `result` right/miss + `rightLabel`/`missLabel` · `disabled` · `onclick` · `testid` — the large answer option of Learn, Test and Cards' multiple choice | `<OptionTile kbd="1" label={o} result={…} onclick={() => pick(i)} />` |

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
| **Card** | `padding` none/sm/md(default)/lg · `elevated` · `onclick` (whole card becomes a button) · `label` · `as` div/section/article/li · `testid` (`data-testid`) | `<Card padding="lg">…</Card>` |
| **Chip** | `label` / children · `tone` · `icon` · `size` sm/md · `selected` + `onclick` (toggle, `aria-pressed`) · `onremove` (× button) | `<Chip tone="accent" label="B1" />` |
| **Badge** | `count` (hidden when 0/null; `max` → "99+") · `tone` · `variant` solid/soft · `label` (accessible text) · children | `<Badge count={n} label={t('…', { count: n })} />` |
| **Avatar** | `name` (initials) · `src?` · `size` · `label?` (decorative without) | `<Avatar name={profile} />` |
| **Kbd** | `keys: string[] \| 'Mod+K'` — `Mod` = ⌘ on macOS, Ctrl elsewhere · `size` sm (18 px, in buttons/fields)/md — mono | `<Kbd keys={['Mod', 'K']} />` |
| **StatTile** | `label` · `value` (formatted) · `unit?` · `hint?` · `icon?` · `tone` · `size` sm/md · children (sparkline…) · `testid` — big mono figure | `<StatTile label={t('…')} value={formatCount(n)} hint={t('…')} />` |
| **Icon** | `icon` (from `icons.ts`) · `size` (16/20/24) · `strokeWidth` · `label?` (decorative without) | `<Icon icon={Search} size={20} />` |
| **ProgressBar** | `label` (required) · `value` · `max` · or `segments: {value,label,tone}[]` (stacked, labels in the accessible name) · `tone` · `size` · `showValue` | `<ProgressBar label={t('…')} segments={[{ value: 3, label: t('…'), tone: 'success' }]} />` |
| **ProgressRing** | `label` · `value` · `max` · `size` px · `stroke` · `tone` · children (centre content, default %) | `<ProgressRing label={t('…')} value={score} />` |
| **EmptyState** | `title` · `description?` · `icon?` · `action?: {label,onclick,icon?}` (one action) · `size` sm/md | `<EmptyState title={t('…')} action={{ label: t('…'), onclick: create }} />` |
| **Skeleton** | `width` · `height` · `shape` text/control/card/circle · `lines` | `<Skeleton shape="card" />` |
| **Spinner** | `size` (decorative; announce loading in text) | `<Spinner />` |
| **ListRow** | `label` · `sub?` · `onclick` (row button) · `buttonLabel?` · `depth` · `expanded?` + `ontoggle` (tree chevron; `toggleTabbable={false}` when the list expands/collapses with →/←) · `active` · `selected` · `header` · `cellColumns` (grid tracks of `cells`) · `leading` / children / `cells` / `actions` snippets (actions show on hover/focus) · `testid` — one dense 34 px row of a list or tree-table, inside a `<ul>` | `<ListRow label={d.name} onclick={open} cellColumns="56px 56px" depth={1}>{#snippet cells()}<span class="num">{due}</span>{/snippet}</ListRow>` |
| **Table** | `rows` · `columns: TableColumn<T>[]` (`{id,header,width?,align?,numeric?,value?}`; `numeric` = mono tabular cell) · `rowKey` · `label` · `rowHeight` (fixed, 34) · `height` (CSS) · `bind:activeKey` · `selectedKeys` (selected rows keep their tint under hover and the active row) · `onrowclick` · `cell` snippet `(row, column)` · `empty` snippet — virtualised grid; ↑/↓ PgUp/PgDn Home/End, Enter | `<Table rows={cards} {columns} rowKey={(c) => c.id} label={t('…')} onrowclick={open} />` |
| **NumberedRow** | `n` · `invalid` · `error` · `actions` snippet · children (the row's fields) — one numbered card of an editable list (Create's rows), inside an `<ol>` | `<NumberedRow n={i + 1}>…{#snippet actions()}…{/snippet}</NumberedRow>` |
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
| **PageHeader** | `title` (the page's `<h1>`) · `subtitle?` · `back?: {label,onclick}` (a sub-page: the title also goes into the top bar's trail, whose page crumb then links back) · `parents?: Crumb[]` (crumbs before the title in that trail, e.g. a sub-deck's parent decks) · `meta` snippet (chips) · `actions` snippet · `tabs` snippet · `chrome` (default true: also draws the shell's deck picker among the actions and the section tabs under the title — `page-chrome.ts`; `false` on sub-pages like the deck page) | `<PageHeader title={deck.name}>{#snippet actions()}<Menu …/>{/snippet}</PageHeader>` |
| **Tabs** | `tabs: TabItem[]` (`{id,label,icon?,badge?,disabled?}`) · `bind:value` · `label` · `variant` line/pill · `onchange(id)` · children snippet `(id)` (panel) | `<Tabs label={t('…')} {tabs} bind:value={tab} />` |
| **Sidebar** | `label` · `collapsed` · `header` / children (NavItems) / `footer` snippets — icon-only below 1024 px | (app shell) |
| **NavItem** | `label` · `icon` · `active` · `accent` (accent-coloured icon) · `badge` (mono count) · `badgeLabel` · `collapsed` · `onclick` · `leading` snippet — 30 px rows | (app shell) |

## Shell (`src/lib/shell/`)

- Layout (v1.2): 216 px sidebar (`AppSidebar`) | a column with the 44 px **top bar**
  (`TopBar.svelte`) over `main .page`. The top bar shows the trail (section › page, plus what
  the screen appends with `getPageChrome()?.setTrail(crumbs, up)` — `PageHeader` with `back`
  does it for you), the search-or-command field (opens the palette; the Search page when the
  palette module is off) and **Kart ekle** (`N`).
- `commands.svelte.ts` — shell entry points: `openQuickAdd({ deckId? })` (top bar button, `N`, `Ctrl+N`;
  `deckId` preselects a deck, e.g. a Library row's "add card";
  the shell's fallback opens the Create page until the Quick Add dialog calls
  `setQuickAddHandler`), `openPalette()` (ShortcutHost registers it), `shellCommands`.

- `navigation.ts` — the only place mapping module routes to the 5 sections, the Inbox or a
  Settings group (`PLACEMENTS`). A new module screen: add its route there. Order within a
  section/group = order in that list.
- Sections with more than one tab page show them as tabs above the screen (`tab: false` in
  `PLACEMENTS` keeps a page out of the tabs — the study modes, opened from the deck page tiles
  and Home); Settings shows a grouped sub-navigation. Home is `features/home/HomeScreen.svelte`.
- A study screen calls `ctx.setFocus(true)` while a session runs (not on its end screen): the
  shell hides the sidebar, the top bar, the deck picker and the tabs and the page spans the
  window; the screen draws `features/modes/SessionFrame` (slim bar: progress, "12 / 40", Esc
  exit, a menu button that brings the sidebar back via `html.fc-session-nav`) and centres its
  column. It resets on every page change. Focus mode (`F`, FocusToggle) adds full screen and
  hidden counters (by setting).
- The deck picker (`deckPicker: true`) and the section tabs are not drawn above the screen: the
  shell hands them to the screen's `PageHeader` (`setPageChrome`), so the title comes first.
- Settings below 1024 px: the grouped sub-navigation collapses into one Select on top.
