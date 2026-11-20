# Flashcard user guide

This guide covers everyday use of the Flashcard desktop app. The app runs on your computer, your data stays on your computer, and it sends no telemetry.

Names in bold (for example **Decks**, **Study**) are the menu and button names in the app. If you use the app in Turkish, see the [Turkish guide](tr.md).

## Contents

1. [Install](#install)
2. [First run](#first-run)
3. [Decks](#decks)
4. [Classifications (levels, exams)](#classifications-levels-exams)
5. [Adding cards and note types](#adding-cards-and-note-types)
6. [Study](#study)
7. [Query language](#query-language)
8. [Import and export](#import-and-export)
9. [Plugins](#plugins)
10. [AI](#ai)
11. [MCP](#mcp)
12. [Browser extension](#browser-extension)
13. [Backup and restore](#backup-and-restore)
14. [Troubleshooting](#troubleshooting)

---

## Install

1. Download the Windows installer from the project's releases page.
2. Run it and follow the steps.
3. When it finishes, open Flashcard from the Start menu.

Windows is the first platform. macOS and Linux builds come later.

## First run

The app opens with an empty collection. The menu is on the left, the selected screen in the middle. The menus you will use most:

| Menu | What it is for |
|---|---|
| **Decks** | Deck tree: create, rename, delete |
| **Add** | Add a new note (and its cards) |
| **Notes** | Notes of the selected deck |
| **Study** | Spaced repetition session |
| **Learn**, **Test** | Learn mode and practice test |
| **Browse** | Search with the query language, bulk changes |
| **Import / export** | Files in and out, backup, printing |
| **Settings** | Language and theme |

The menu also has **Deck options**, **Note types**, **Statistics**, **Planning**, **Goals**, **Reading**, **Inbox**, **Automation**, **AI**, **MCP**, **Plugins**, **Maintenance** and **Shortcuts**. A module that is turned off has no menu entry.

**Language and theme:** on the **Settings** screen choose **Language** (Türkçe / English) and **Theme** (**System**, **Light**, **Dark**).

**Command palette:** open it with `Ctrl+K`. Type a command or screen name and press **Run**. Some commands need values in the **Parameters (JSON)** box. In this version this is also how you turn modules on or off and change settings:

- **Turn a module on/off**: `{"id": "local_api", "enabled": true}`
- **Change a setting**: `{"scope": {"kind": "app"}, "key": "timed.seconds", "value": 20}`

**Global shortcuts** (all can be changed on the **Shortcuts** screen):

| Key | Action |
|---|---|
| `Ctrl+K` | Open command palette |
| `Ctrl+F` | Search cards |
| `Ctrl+N` | New note |
| `Ctrl+Z` | Undo |
| `Ctrl+Shift+Z` | Redo |

Almost every change can be undone: adding or deleting notes, ratings, bulk changes, imports.

## Decks

On the **Decks** screen:

- **New deck** creates a top-level deck.
- **New subdeck** adds a deck under the selected one. Example tree: `Languages` → `English` → `A1`.
- **Rename** and **Delete** change a deck. A deleted deck goes to the trash with its subdecks, notes and cards.

Get a deleted deck or note back with **Maintenance** → **Trash** → **Restore**. By default, items are deleted for good after 30 days in the trash.

In queries, deck paths use `::`: `deck:"Languages::English"`. Selecting a deck includes its subdecks.

**Deck options:** values such as new cards per day, maximum reviews per day, learning steps, **Desired retention** and **Maximum interval (days)** live in a preset. Create one with **New preset** and assign it with **Use for deck**. A deck uses its own preset, else its parent deck's, else the defaults.

## Classifications (levels, exams)

Tags are free-form. A classification is a structured field: it has a name and options, belongs to a deck and applies to all its subdecks.

Examples:

- In an `English` deck, **Level** = A1 · A2 · B1 · B2 · C1 · C2
- In a `YKS` deck, **Exam** = TYT · AYT · YDT
- In a `Japanese` deck, **JLPT** = N5 … N1

Built-in templates:

| Template id | Name | Options |
|---|---|---|
| `cefr` | Level | A1–C2 |
| `yks` | Exam | TYT, AYT, YDT |
| `jlpt` | JLPT | N5–N1 |
| `hsk` | HSK | HSK 1–HSK 6 |
| `school_grade` | Grade | Grade 1–Grade 12 |
| `difficulty` | Difficulty | Easy, Medium, Hard |

A classification copied from a template is fully yours: add, rename, reorder and merge options. Saved queries keep working after a rename.

**Defining one.** This version has no separate screen for defining classifications. There are two ways:

- Command palette (`Ctrl+K`) → **Classification from template** → `{"template_id": "cefr"}`. This defines it for the whole collection.
- For one deck, with the `fc` command-line tool: `fc attribute apply-template cefr --deck Languages --lang en`.

With `--lang tr` the names are Turkish (**Seviye**, **Sınav**), so queries like `seviye:B1` and `sınav:TYT` work. The key (`level`, `exam`) works in both languages.

**Using them.**

- When you add a note, the deck's classifications appear next to the fields. Mandatory ones are marked **Required**.
- In queries: `level:B1`, `level>=B1` (for ordered options), `exam:TYT`, or with Turkish names `seviye:B1`, `sınav:TYT`. Keys ignore case and accents: `sinav:TYT` works too.
- **Browse** → change selection → **Classification** sets a value in bulk.
- On import you can map a column to a classification (see below).
- **Statistics** has a **By classification** breakdown.
- A custom session, learn mode and practice test can target one: `level:A2`, `exam:TYT`.

## Adding cards and note types

Flashcard separates **notes** from **cards**. A note is the content (fields). Cards are generated from it by its note type. For example, a **Classic + reverse** note makes two cards.

**Adding a note:**

1. Choose a deck on the left.
2. Go to **Add** and pick the **Note type**.
3. Fill in the fields, then any classifications and **Tags**.
4. Press **Add**. You see "Note added."

Fields accept Markdown, `$math$` (KaTeX) and `[[links]]`. Drop or paste images, audio or video. Adding the same file twice keeps a single copy on disk.

**Built-in note types:**

| Note type | In short |
|---|---|
| Classic | Front → back, you grade yourself |
| Classic + reverse | Two cards, both directions |
| Optional reverse | Reverse card only if you want it |
| Type the answer | You type, the app checks |
| Cloze | `{{c1::Ankara}} is the capital`; each number is a card, hint with `{{c1::Ankara::city}}` |
| Multiple choice | Pick one of the options |
| Image occlusion | Draw masks on an image; one card per mask |
| Image quiz | The image is the question |
| Listening card | Audio is played |
| Vocabulary | Word, meaning, example sentence |
| Sentence card | Sentence, translation, target word |

On the **Note types** screen create one with **New note type** or **Copy** a built-in one. Add fields (**Add field**), card templates (**Add template**) and edit **Styling (CSS)**. The template language has `{{field}}`, `{{#field}}…{{/field}}`, `{{cloze:field}}`, `{{type:field}}`, `{{hint:field}}`, `{{tts:en:field}}` and `{{FrontSide}}`. Adding or removing a field does not break existing cards or their history.

**Quick cards while studying:** on the study screen, `N` opens **New cards from text**. Each line becomes a card: `porsuk :: badger`. The session keeps going, and all cards are undone in one step.

## Study

On **Study**, choose a deck on the left (or none for **All decks**) and press **Study now** or `Enter`. The app uses the FSRS scheduler.

**Ratings.** After you see the answer, pick one of four buttons. Each button shows when the card comes back (`10m`, `3d` and so on).

| Button | Key | Meaning |
|---|---|---|
| **Again** | `1` | I did not know it |
| **Hard** | `2` | I struggled |
| **Good** | `3` | I knew it |
| **Easy** | `4` | Too easy |

**Study shortcuts:**

| Key | Action |
|---|---|
| `Space` or `Enter` | Show answer; on the answer, rate **Good** |
| `1`–`4` | Rate |
| `1`–`9` | Pick an option on a multiple-choice card |
| `H` | Hint |
| `S` | Suspend |
| `B` | Bury (until tomorrow) |
| `Z` | Undo last answer |
| `Ctrl+1`–`Ctrl+7` | Toggle a flag |
| `N` | New cards from text |
| `F` | Focus mode |
| `Esc` | End session |
| `R` (on the summary) | Study mistakes again |

`Space`, `1`–`4`, `H`, `S`, `B`, `Z` and `Esc` can be changed on the **Shortcuts** screen.

**Options before you start:**

- **Practice mode (does not change the schedule)**: card schedules stay as they are.
- **Custom session**: type a query such as `level:A2 is:due` in **Query**, or pick a **Saved query**.
- **Postpone today's cards**: enter the number of days and press **Postpone**.

When the session ends, the **Session complete** screen shows cards studied, accuracy, time and mistakes.

### Modes

| Mode | Where | What it does |
|---|---|---|
| Learn | **Learn** menu | Every card is asked as multiple choice, then written. A card is learned once you get it right at both steps. A wrong written answer goes back to multiple choice. |
| Test | **Test** menu | A practice test with mixed questions from the chosen cards. It does not change your schedule. |
| Confidence | Study → **Rating** → **Confidence 1–5** | Instead of four buttons you answer "How sure were you?" from 1 to 5. |
| Timed | Study → **Time limit** | A time limit for every card. |
| Focus | Study → **Focus** button or `F` | Full screen with distractions hidden. |

**Learn mode:** choose a deck, optionally type `level:A2` in **Query**, press **Start learning**. Keys: `1`–`4` pick, `Enter` check / continue, `Z` undo, `Esc` end.

**Practice test:** choose **Questions** and **Question types** (**Multiple choice**, **Written**, **True or false**), optionally type `exam:TYT` in **Query**, press **Create test**. Print the paper with **Print** or **Print answer key**. **Submit** scores it; "almost" counts as half a point. Submitting can be undone.

**Confidence rating:** the default mapping is 1–2 = Again, 3 = Hard, 4 = Good, 5 = Easy. Keys `1`–`5` work on the answer side. Change the mapping with the settings `confidence.map.1` … `confidence.map.5`.

**Timed mode:** enter the time in **Seconds**. For **When time runs out** choose **Count as Again** or **Skip**. The timer starts when the card appears, runs per card and cannot be restarted. An answer after the time is up is not accepted.

**Focus mode:** the settings `focus.fullscreen`, `focus.hide_counts` and `focus.auto` control full screen, hiding counters and starting every session in focus mode.

Practice modes (practice mode, tests) are not counted in statistics. Each mode is its own module and can be turned off.

**Other study settings** (by setting key): `study.scheduler` (FSRS or SM-2), **Answer checking** (`study.answer_checker`: exact, ignore case and punctuation, allow typos, several answers), **Treat ı/i, ş/s… as equal** (`study.fold_turkish`), **Hint style** (`study.hint_strategy`).

## Query language

The query language is the same in **Browse**, custom sessions, learn/test, automation and MCP. Terms separated by spaces are combined with AND.

| Syntax | Meaning |
|---|---|
| `sword` | Word in the note text (prefix match) |
| `"sword shield"` | Exact phrase |
| `deck:English` · `deste:English` | Deck (subdecks included); `deck:"Languages::English"` |
| `tag:verb` · `etiket:verb` | Tag (subtags included); `tag:none` = untagged |
| `notetype:Vocabulary` · `note:Classic` | Note type name |
| `type:cloze` | Card type: `classic`, `typing`, `cloze`, `multiple_choice`, `image_occlusion` |
| `card:` | Card template |
| `is:new` `is:learn` `is:review` `is:relearn` | Card state |
| `is:due` `is:suspended` `is:buried` `is:flagged` | Due, suspended, buried, flagged |
| `flag:red` · `flag:1` | Flag: 1–7, a colour name (EN/TR) or your own flag name; `flag:0` / `flag:none` unflagged, `flag:*` any |
| `stability>30` `difficulty>7` `lapses>=5` `reps<3` | Numbers; `=` `!=` `<` `<=` `>` `>=` |
| `retrievability<0.8` | Recall probability right now (0–1) |
| `added:7` `edited:7` `reviewed:1` | Added / edited / reviewed in the last N days |
| `due:3` | Due within N days |
| `level:B1` · `level>=B1` · `exam:TYT` | Classification (key or name); `level:*` = has a value |
| `word:app*` | Field search; `*` is a wildcard |
| `re:^ab` · `word:re:^ab` | Regular expression (all fields / one field) |
| `nid:` `cid:` | Note / card ids (comma-separated) |
| `-tag:easy` · `NOT tag:easy` | Not |
| `a OR b` · `(level:B1 OR level:B2)` | Or, grouping |
| `level!=B1` | Same as `-level:B1` (notes without a value match too) |

For a bad query the app shows where the error is. On **Browse**, name a query with **Save query**; saved queries can also be picked as **Saved query** on the study screen.

## Import and export

Everything is on the **Import / export** screen. It has four tabs: **Import**, **Export**, **Backup**, **Print**. Choose a deck on the left first.

### Text and CSV

1. **Import** → **Source**: **File** or **Paste text**.
2. **Format**: leave **Automatic** or choose **Text / CSV**.
3. The app finds the separators and says "Detected automatically (…%)". All of these are recognised:

```
dog, köpek
cat: kedi
Arda	porsuk
"Arda, Ali": porsuk
```

4. Check the **Preview** table. If columns are split wrong, fix it:
   - Type the right separator in **Between term and definition** (`,` `;` `:` ` - ` `~` ` / ` and so on; `\t` for a tab; two spaces for columns lined up with spaces).
   - **Between cards**: **New line**, **Blank line** (for multi-line definitions) or `;`.
   - Tick **First row is a header** if there is one.
5. Above each column choose what it becomes: a field, **Tags**, a classification, or **Don't import**.
6. **If a note already exists**: **Skip it**, **Update it** or **Add anyway**.
7. If a classification column has unknown values, tick **Create missing classification options**.
8. Press "Import into …".

The file encoding is detected (UTF-8, UTF-16, Turkish Windows-1254). Anki's `#separator:` header line is read too. Auto-detection is not right for every file; the preview always lets you fix it by hand.

### Quizlet

Paste Quizlet's export text and choose **Quizlet** as **Format**. Default: tab between term and definition, new line between cards. If you chose custom separators in Quizlet, type the same ones in the preview.

### Anki (.apkg, .colpkg)

- **Import:** choose an **Anki (.apkg, .colpkg)** file. The preview shows the number of notes, cards, media files and decks. With **Keep review history and scheduling** ticked, cards keep their Anki schedule. Cloze, image and audio cards are supported; old and new Anki package layouts are read.
- **Export:** **Export** → **Anki deck (.apkg)** or **Anki collection (.colpkg)**. **Include review history and scheduling** exports the schedule too. Packages are written in the older schema that every Anki since 2.0 imports. Exported packages are tested by opening them in real Anki (the official `anki` library, 26.09): notes, cards, scheduling, history, media and image occlusion masks arrive unchanged.

### JSON

A note format you can edit by hand:

```json
{ "format": "fc-json", "version": 1,
  "notes": [ { "deck": "English::A1", "notetype": "Classic",
               "fields": { "front": "dog", "back": "köpek" },
               "tags": ["animals"], "attributes": { "level": "A1" } } ] }
```

Without `notetype`, Classic is used. Fields and classifications are given by key or name. To export, choose **JSON (.json)**.

### Deck package (.fcpack)

The least lossy way to move a deck to another Flashcard: notes, cards, media, classifications and (if you want) history travel together. The package keeps note ids and classifications exactly.

### Export and print

- **Export** writes the selected deck with its subdecks; with no deck selected, the whole collection. Formats: **Deck package (.fcpack)**, **Anki deck (.apkg)**, **Anki collection (.colpkg)**, **Text (.txt)**, **JSON (.json)**.
- On the **Print** tab choose a **Layout** (**Cards, 2 × 4 per page**, **Cards, 3 × 5 per page**, **List**), press **Prepare**, then **Print / save as PDF**. Backs are mirrored for double-sided printing.

Long jobs can be cancelled with **Cancel**; nothing changes when you cancel. A finished import is undone in one step (**Undo import** or `Ctrl+Z`).

## Plugins

Plugins run in a sandbox (WASM) and can only do what you allow. They can add card types, study modes, voice providers, or hooks that fill fields when a note is added.

**Installing:**

1. **Plugins** → **Install from file…** → choose a `.fcplugin` file.
2. The dialog shows the plugin's name, version, what it adds and the permissions it asks for.
3. Untick what you do not want to allow. Items marked "asks separately" are sensitive permissions.
4. Press **Install**.

Each plugin has **Turn on** / **Turn off**, **Permissions**, **Settings**, **Log** and **Uninstall**. When uninstalling you can tick **Also delete its saved data**; notes it created stay.

A plugin that hangs or crashes does not take the app down: calls are cut off by time, work and memory limits. A plugin that fails several times in a row is switched off ("Switched off after errors"); turn it back on with **Turn on**.

There is no plugin store yet; you install plugins from files.

## AI

The **AI** screen has three tabs: **Make cards**, **Providers**, **Usage**.

**Add a provider:** **Providers** → **Add provider**. Types: **Anthropic Claude**, **OpenAI**, **OpenAI-compatible**, **Ollama (local)**. Enter the API key and press **Save key**. The key is stored only in the operating system's keychain, never in the database or a backup. With **Model per feature** you can, for example, use a strong model for making cards and a fast one for checks.

**Make cards:**

1. Choose a deck on the left.
2. **Source from**: **Text**, **PDF** (with a page range) or **Web page**.
3. Set **Note type**, **How many** and optionally **Extra instructions** (for example "focus on verbs").
4. **Preview**. The **Before sending** dialog shows provider, model, estimated tokens and cost. **Show exactly what will be sent** shows the full text.
5. **Send**. Take the suggestions you want with **Add**.

If the deck has a level classification, the cards are made for that level.

**While studying:** the study screen shows **AI hint**, **Explain** and **Check with AI** buttons.

**Cost:** the **Usage** tab shows requests this month and in the last 7 days. Set a monthly budget with the `ai.monthly_budget_usd` setting. Costs are estimates from the price table; keep the **Prices** table current. With Ollama the model runs on your computer and nothing leaves it.

## MCP

MCP lets assistants such as Claude Desktop and Claude Code work with your collection. The **Use from Claude** part of the **MCP** screen shows the setup snippets to copy, the tool list and the write switch.

**Setup:** the server runs through the `fc` command-line tool and opens your collection file (`collection.db`). Replace the path with your own file.

Claude Code:

```bash
claude mcp add flashcard -- fc --db "C:\Users\<you>\AppData\Roaming\app.flashcard.desktop\collection.db" mcp serve
```

Claude Desktop (`%APPDATA%\Claude\claude_desktop_config.json`; write `\` twice inside JSON):

```json
{ "mcpServers": { "flashcard": {
    "command": "fc",
    "args": ["--db", "C:\\Users\\<you>\\AppData\\Roaming\\app.flashcard.desktop\\collection.db", "mcp", "serve"] } } }
```

If `fc` is not on your PATH, use its full path.

**Tools:** **List decks**, **List note types**, **List classifications of a deck**, **Search cards**, **Read a note**, **Create a deck**, **Add notes**, **Edit a note**, **Set a classification (asks for confirmation)**. No tool deletes anything.

- Every change an assistant makes is an ordinary step in the app; `Ctrl+Z` undoes it.
- With **Let assistants make changes** off, assistants can only read. `mcp serve --read-only` does the same for one connection.
- Under **External MCP servers**, **Add server** adds other servers whose tools the app's AI features may use. The app shows the program it will start and asks you to confirm.

## Browser extension

For Chromium-based browsers such as Chrome, Edge and Brave. It sends a word or sentence you select on a page to the app's **Inbox**.

**Setup:**

1. Install the extension. Load the unpacked build via `chrome://extensions` → **Developer mode** → **Load unpacked** and choose the extension folder.
2. In the app, turn on the **Local API** module (off by default): `Ctrl+K` → **Turn a module on/off** → `{"id": "local_api", "enabled": true}`. **Local API** appears in the menu. The default port is `28735`.
3. Extension button → **Connect**. The app opens a **Connection request** dialog; allow it.

**Use:**

1. Select text on a page → right-click → **Add to deck** (or `Alt+Shift+A`, or the toolbar button).
2. Fill in **Front**, **Back**, **Deck** and the deck's **Classifications** (for example Level).
3. **Add to inbox**.
4. In the app: **Inbox** → edit the suggestion → **Accept** or **Reject**. Nothing becomes a card until you accept it.

Items added while the app is closed wait in the extension and are sent when the app opens. The extension only connects to `127.0.0.1`. If you changed the port, enter the same value in **Local API port** in the extension's settings. You can see and revoke connected programs on the **Local API** screen.

The inbox also receives suggestions from `flashcard://` links and the `Ctrl+Alt+N` quick-add window.

## Backup and restore

**Import / export** → **Backup**:

- **Create backup…** writes the whole collection to one `.fcbackup` file: notes, cards, history, classifications, settings and media.
- **Restore backup…** asks for a backup, shows its date and note count and asks you to confirm. The current collection is backed up first, then replaced. The message shows where that safety backup went (the `backups` folder in the collection folder).

Good to know:

- After a restore, undo history starts over; earlier steps cannot be undone.
- A backup from a newer app version cannot be restored into an older one.
- Programs connected to the Local API lose their permission on restore and must connect again.
- On Windows the collection file is `%APPDATA%\app.flashcard.desktop\collection.db`. Do not copy it by hand while the app is open; use **Create backup…**.

## Troubleshooting

**Columns came out wrong on import.**
In the preview, type the right separator in **Between term and definition** (`\t` for tab) and change **Between cards** if needed. If you already imported, use **Undo import** or `Ctrl+Z`.

**I deleted something by mistake.**
Undo with `Ctrl+Z`. If a deck or note went to the trash, use **Maintenance** → **Trash** → **Restore**.

**It says "Undo stops here".**
The collection was replaced from a backup or the trash was emptied for good. Earlier history cannot be undone.

**"Nothing to study right now."**
Today's cards are done. You can still study with **Practice mode** or by typing a query (`deck:English`) in **Custom session**.

**The browser extension says the app is not reachable.**
Check that the app is open, the **Local API** module is on and the port matches the extension's. The item is not lost; it waits in the extension.

**Claude cannot change my collection.**
Turn on **Let assistants make changes** on the **MCP** screen and make sure you are not using `--read-only`. The server does not open a collection with an older schema: open the app once first.

**AI says the provider needs an API key.**
Save the key under **AI** → **Providers**. If you see "No system keychain", keys cannot be stored on this system.

**A plugin shows "Switched off after errors".**
Check its **Log** and **Last error**, then try **Turn on** again. If it keeps failing, uninstall it.

**A menu is missing.**
Its module may be off. Check with `Ctrl+K` → **List modules** and turn it on with **Turn a module on/off**.

**I think the database has a problem.**
**Maintenance** → **Database check** → **Check**. If it finds problems, press **Repair**; the repair can be undone too.
