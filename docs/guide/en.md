# Flashcard user guide

This guide covers everyday use of the Flashcard desktop app. The app runs on your computer, your data stays on your computer, and it sends no telemetry.

Names in bold (for example **Library**, **Study**) are the menu and button names in the app. If you use the app in Turkish, see the [Turkish guide](tr.md).

**What changed in v1.3?** For a short, friendly tour see [What's new in v1.3](v1.3-en.md).

## Contents

1. [Install](#install)
2. [First run](#first-run)
3. [Decks](#decks)
4. [Classifications (levels, exams)](#classifications-levels-exams)
5. [Adding cards and note types](#adding-cards-and-note-types)
6. [Study](#study)
7. [Statistics and planning](#statistics-and-planning)
8. [Query language](#query-language)
9. [Import and export](#import-and-export)
10. [Plugins](#plugins)
11. [AI](#ai)
12. [MCP](#mcp)
13. [Browser extension](#browser-extension)
14. [Automation](#automation)
15. [Profiles and app lock](#profiles-and-app-lock)
16. [Reminders and tray](#reminders-and-tray)
17. [Crash recovery and logs](#crash-recovery-and-logs)
18. [Data folder](#data-folder)
19. [Backup and restore](#backup-and-restore)
20. [Troubleshooting](#troubleshooting)

---

## Install

Windows is the first platform. macOS and Linux builds come later. The project's releases page has two installers:

| File | For |
|---|---|
| `Flashcard_<version>_x64-setup.exe` | Recommended. Installs for your user only, no admin rights, into `%LOCALAPPDATA%\Flashcard\`. Installer in Turkish or English. |
| `Flashcard_<version>_x64_en-US.msi` | Optional, for managed installs. Per machine, asks for admin rights, installs into `C:\Program Files\Flashcard\`. |

1. Download the `-setup.exe` and run it.
2. Windows SmartScreen says "Windows protected your PC": click **More info** → **Run anyway**. The installers are not code-signed, so this warning is expected.
3. If WebView2 is missing, the installer downloads and installs it on its own.
4. When it finishes, open Flashcard from the Start menu.

The installer contains only the desktop app. The `fc` command-line tool is not included (see [MCP](#mcp)).

**Updates.** The app contacts the update server only when you ask: **Settings** → **Advanced** → **Logs & updates** → **Check for updates**. If a new version is out, press **Download** ("Downloading and verifying…"), then **Install and restart**. The app closes, installs the update without questions and starts again; your data stays. Every update is signed with the project's key; a download with a wrong signature is refused and nothing is installed. Nothing is installed without your click. To check at every start, turn on the setting `updates.check_on_start` (off by default): `Ctrl+K` → **Change a setting** → `{"scope": {"kind": "app"}, "key": "updates.check_on_start", "value": true}`. A found update then shows a "Version … is available." banner with **See update**.

**Uninstall:** Windows *Settings* → *Apps* → Flashcard → *Uninstall*. Your data folder (see [Data folder](#data-folder)) stays unless you tick the uninstaller's box to delete the application data; that deletes all profiles, settings and logs for good.

## First run

On an empty collection the app opens with **Welcome to Flashcard**: "Four short steps to get going; each one does the real thing."

1. **What are you learning?**: pick a subject (**English**, **Japanese**, **Chinese**, **Another language**, **YKS (TYT / AYT / YDT)**, **Another exam**, **School (grades 1–12)**, **General knowledge**). Each one shows the classification it sets up, for example **Classification: CEFR levels (A1–C2)**. Check the **Deck name** and tick any **Starter decks** you want.
2. **Start with a ready deck** adds the ticked starter decks; **I’ll add my own words** sets up an empty deck.

The setup creates a deck with that classification, a fitting note type (**Vocabulary** for languages, **Classic** otherwise), a deck options preset for languages ("Language learning", 15 new cards a day) and exams ("Exam preparation", 30 new cards a day), and the chosen starter decks. All of it is one undo step: `Ctrl+Z` removes the whole setup. **Skip the guide** closes the dialog. A collection that already has cards never shows it.

Afterwards a small **Getting started** checklist stays at the bottom right. Each step's button does the thing: **Add your first words**, **Study**, **Where today’s cards and your progress live** (**Home**, **Statistics**) and the optional **Connect the browser extension**. **Hide** folds it away, **Finish the guide** closes it. Open it again with **Open the getting-started guide** on the **Starter decks** page.

**Starter decks**: small decks written for this app, in Turkish or English (**Language of the cards**): basic English words (A1–A2), YKS TYT math basics, world capitals and an app guide. **Add** puts one into your collection as an ordinary deck you can change or delete.

**The layout.** The left side has five sections; a section's pages show as tabs at the top:

| Section | What is in it |
|---|---|
| **Home** | How many cards to study today, **Start studying**, streak, goal, decks to study today |
| **Library** | **Decks** (deck page, study modes) and **Notes** |
| **Create** | **Add** (the Add cards page), **AI**, **Starter decks** |
| **Search** | **Browse**: search with the query language and bulk changes |
| **Statistics** | **Overview**, **Planning**, **Goals** |

The **Inbox** (with the number of waiting proposals) and **Settings** sit at the bottom left. A module that is turned off has no page anywhere.

**Settings.** Settings are in five groups, by what you want to do:

| Group | Pages |
|---|---|
| **General** | Language, **Appearance** (colour scheme, **Theme**, **Accent colour**), **Classifications**, **Reminders**, **Shortcuts**, **Profiles & lock** |
| **Studying and cards** | **Deck options**, **Note types**, **Modes and reading** |
| **Connections** | **AI**, **Subtitle** |
| **Data** | **Export & backup**, **Starter decks** |
| **Advanced** | "Technical settings: you rarely need these." **Automation**, **Assistants (MCP)**, **Local API**, **Plugins**, **Maintenance**, **Logs & updates** |

Settings explain themselves: under every row it says what it does and what changing it means, often with an **Example:**. The default value is marked **default**; **Reset to default** undoes a change to a row. Rows you rarely need are under **Advanced settings** at the bottom of each page (whether it is open is remembered). Not sure which page a setting is on? Type in **Search settings**: matching rows are highlighted and **Advanced settings** opens by itself when needed. The **General** page starts with **What do you want to change?** (for example "How many new words a day?", "Remind me to study every day"); click one to open the right page.

**Command palette:** open it with `Ctrl+K`. Type a command or screen name and press **Run**. Some commands need values in the **Parameters (JSON)** box. This is how you turn modules on or off and change settings that have no row on a screen:

- **Turn a module on/off**: `{"id": "local_api", "enabled": true}`
- **Change a setting**: `{"scope": {"kind": "app"}, "key": "timed.seconds", "value": 20}`

**Global shortcuts** (all can be changed under **Settings** → **General** → **Shortcuts**):

| Key | Action |
|---|---|
| `?` | Keyboard shortcuts list |
| `Ctrl+K` | Open command palette |
| `Ctrl+F` | Search cards |
| `N` or `Ctrl+N` | Add card (quick add window) |
| `Ctrl+Z` | Undo |
| `Ctrl+Shift+Z` | Redo |

**The `?` shortcuts sheet.** When you are not typing, press `?` (or the **?** button in the top bar, or **Keyboard shortcuts** in the palette): every shortcut that applies right now opens in a searchable list. **Change shortcuts** takes you to the **Shortcuts** page. Shortcuts do not fire inside a text field: `Ctrl+Z` and `Ctrl+A` undo and select the text there and never touch your cards. A key held down acts once.

Almost every change can be undone: adding or deleting notes, ratings, bulk changes, imports.

## Decks

Decks live under **Library** → **Decks** (**Cards** or **Tree** view):

- **New deck** creates a deck. Use `::` in the name for a subdeck, for example `Languages::English::A1`.
- Click a deck to open its page: a card preview, **Study modes** (**Flashcards**, **Learn**, **Test**, **Match**, **Reading**), progress and the **Terms in this deck** list.
- The **Deck actions** menu on the deck page has **Rename**, **New subdeck**, **Deck options**, **Export** and **Delete deck**. A deleted deck goes to the trash with its subdecks, notes and cards.

In the term list, **Term** and **Definition** are edited in place. The app knows which field is the term and which the meaning from the field roles of the note type (see [note types](#adding-cards-and-note-types)); for a note type where it cannot tell, the pencil opens the note's full editor instead of guessing.

Get a deleted deck or note back with **Settings** → **Advanced** → **Maintenance** → **Trash** → **Restore**. By default, items are deleted for good after 30 days in the trash.

In queries, deck paths use `::`: `deck:"Languages::English"`. Selecting a deck includes its subdecks.

**Deck options** (**Settings** → **Studying and cards** → **Deck options**): values such as **New cards per day**, **Maximum reviews per day**, learning steps, **Desired retention** and **Maximum interval (days)** live in a **preset**. Create one with **New preset** and assign it with **Use for deck**. A deck uses its own preset, else its parent deck's, else the defaults.

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

**Defining one.** The first-run setup creates one for its deck. Otherwise:

- Command palette (`Ctrl+K`) → **Classification from template** → `{"template_id": "cefr"}`. This defines it for the whole collection.
- For one deck, with the `fc` command-line tool (not included in the installer, see [MCP](#mcp)): `fc attribute apply-template cefr --deck Languages --locale en`.

With `--locale tr` the names are Turkish (**Seviye**, **Sınav**), so queries like `seviye:B1` and `sınav:TYT` work. The key (`level`, `exam`) works in both languages.

**Using them.**

- When you add a card, the deck's classifications appear next to the fields (in quick add under **More**). Mandatory ones are marked **Required**.
- In queries: `level:B1`, `level>=B1` (for ordered options), `exam:TYT`, or with Turkish names `seviye:B1`, `sınav:TYT`. Keys ignore case and accents: `sinav:TYT` works too.
- **Browse** → change selection → **Classification** sets a value in bulk.
- On import you can map a column to a classification (see below).
- **Statistics** has a **By classification** breakdown.
- In the study launcher, **Options** → **Only cards with** or **Query** (`level:A2`, `exam:TYT`) targets a classification.

## Adding cards and note types

Flashcard separates **notes** from **cards**. A note is the content (fields). Cards are generated from it by its note type. For example, a **Classic + reverse** note makes two cards.

### Add cards: four ways, one place

Every way to add cards is on one page: **Create** → **Add** (titled **Add cards**). At the top it asks **How do you want to add** and offers four ways:

| Way | When |
|---|---|
| **One card** | A word or two: opens the quick add window |
| **Bulk** | Fill it in like a grid: one term and its definition per row |
| **Paste** | A list you copied from a spreadsheet, a table or a document |
| **From a file** | A CSV/text, Anki, Quizlet, JSON or deck package file |

All four share the same **Where the cards go** choice (**New deck** or **Existing deck**, and the **Note type**) and end with the same result: "12 cards added" · **Undo** · **Open deck** · **Study now**. If rows were skipped, **Why?** gives the reason for each. **Study now** opens the study launcher on that deck.

**One card (quick add).** It also opens from anywhere with `N`, `Ctrl+N` or **Add card** in the top bar. The word, its meaning and an optional example sentence come first; other fields, classifications and the preview are under **More**. `Enter` moves to the next field and adds on the last one; `Shift+Enter` is a new line. The window stays open for the next word. If the word already exists the window says so right away ("“go” is already there…") and offers **Add anyway**. The **Other ways:** links at the bottom switch to **Bulk**, **Paste** and **From a file** on the chosen deck.

**Bulk.** Rows of **Term** and **Definition**. `Tab` goes to the next field, `Enter` on the last card adds a new one, `Ctrl+Enter` saves (**Create**). You can add an image to a row. Cards that already exist are kept below so you can change them and save again.

**Paste.** Paste your list into the box: one card per line. The app finds the separators itself (tab, comma, semicolon, colon, dash…) and shows live how the cards will look. If a separator is wrong, press **Change** and pick the one **Between term and definition** and **Between cards**. Then **Add N cards**. **Edit in the grid first** moves the list into the **Bulk** grid.

**From a file.** Drop the file or press **Choose a file…**. Steps: **Choose a file** → **Map columns** → **Import**. Details in [Import and export](#import-and-export).

**Columns are mapped by role.** When pasting or importing, choose what each column becomes from its header: a field (by role name: **term**, **meaning**, **example sentence**, **sentence translation**, **source**…), **Tags**, a classification or **Don't import**. The app fills the mapping in for you: from header words (`word`, `kelime`, `meaning`, `anlam`, `sentence`, `örnek`…), else from the mapping you used last with this note type, else in order (term, meaning, example…). It tells you which one it used. So the sentence in `word ⇥ meaning ⇥ sentence` rows lands in the note's example sentence field. If the chosen note type has no example sentence field, the app warns you: "Switch to “Vocabulary” to keep the example sentence". A column that goes to no field is pointed out too.

**If the card already exists**: **Skip (recommended)**, **Update the existing one** or **Add anyway**. Under each choice it says in plain words what will happen (for example, updating keeps your study history).

Fields accept Markdown, `$math$` (KaTeX) and `[[links]]`. Drop or paste images, audio or video. Adding the same file twice keeps a single copy on disk.

### Built-in note types

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
| Vocabulary | Word, meaning, example sentence; **Recognition** and **Production** cards |
| Sentence card | Sentence, translation, target word, meaning, source |
| Subtitle card | Cards from the Subtitle extension: word, as it appeared, meaning, sentence and its translation, source, screenshot |

**The back of a vocabulary card.** The back of Vocabulary, Sentence and Subtitle cards is three separate parts: the **meaning**, largest; below it, labelled **Example**, the sentence and its translation (on its own line); smallest, **Source**. Templates you edited yourself are never changed.

### Note types

**Settings** → **Studying and cards** → **Note types**. The page opens from a learner's point of view: the types as tiles (**Your types**, **Ready-made types**), the chosen type's fields and a live preview of its cards (flip it with **Show the back**). Each type says how many notes use it and how many cards each note becomes.

- **New note type** starts from a ready-made starting point: **Word**, **Word + example sentence**, **Both directions**, **Fill in the blank (cloze)**, **Subtitle** or **Plain: front and back**. You can also duplicate a ready-made type with **Make a copy**.
- **Field roles: "Contains:"** For each field, pick what it holds: **The word (dictionary form)**, **As it appeared (e.g. “went”)**, **Its meaning**, **An example sentence**, **The example sentence's translation**, **Where it came from**, **A note or explanation** or **Something else**. With roles the app does not have to guess: hints, import, the inbox, the term list and statistics all use the right field. A role can be on only one field of a type.
- **What does each card ask for?** For each card template, pick which field is the answer (or **The whole back side**). When you type an answer, and in Test, Learn and Match, your answer is compared with that field. For example, on a "went" card from a subtitle (go → gitmek), typing "gitmek" is enough; you are not expected to type the example sentence and the source. The reverse card asks for the term.
- Template code, styling (CSS) and field details are under **Advanced: template code**. The template language has `{{field}}`, `{{#field}}…{{/field}}`, `{{cloze:field}}`, `{{type:field}}`, `{{hint:field}}`, `{{tts:en:field}}` and `{{FrontSide}}`. Before you save or delete, the app tells you exactly what would be lost. Adding or removing a field does not break existing cards or their history.

Built-in types from v1.2 that you never changed (Vocabulary, Sentence, Subtitle card and the others) get these roles and answer fields by themselves with the update.

**Quick cards while studying:** on the study screen, `N` opens **New cards from text**. Each line becomes a card: `porsuk :: badger`. The session keeps going, and all cards are undone in one step.

## Study

The app uses the FSRS scheduler.

### Starting to study: the launcher

Every way to study opens the same window: **Start studying** on **Home** (or `Enter`), a deck's **Study** button, the mode tiles on the deck page, **Study these** in Statistics, or **Study now** after adding cards. Opened without a deck, the launcher works on **All decks**.

The "N cards to study today" number on **Home**, the launcher's **Today’s cards** count and the cards a session really serves come from the same calculation. The **All decks** queue honours each deck's own new-card and review limits: one deck reaching its limit does not affect the others. When a note's cards sit in different decks (say, the forward and the reverse card), only one of them comes up on the same day; the other waits for tomorrow.

In the launcher, from top to bottom:

1. **Unfinished**: an unfinished session sits at the top and has the focus; **Continue** picks it up where you left off ("12 answered · 8 left").
2. **Mode**: **Cards**, **Learn**, **Test**, **Match**. Below it, what the mode does to your schedule.
3. **What to study**: five scopes, each showing exactly how many cards it will bring:

   | Scope | What it brings | Schedule |
   |---|---|---|
   | **Today’s cards** | Due reviews and today’s new cards (within the daily limits) | Changes |
   | **New cards** | Cards you have never studied, within the daily limit | Changes |
   | **Studied today** | Every card you answered today (in any mode) | **practice · schedule unchanged** |
   | **Struggled with today** | Cards you rated "Again" or got wrong today | **practice · schedule unchanged** |
   | **All cards** | Every card in the deck | **practice · schedule unchanged** |

   An empty scope cannot be picked and says why: "This deck has no cards yet", "Nothing is due today", "Today's card limit is reached (new card and review limits)", "Next card in 5 min", "You haven’t studied today yet", "Nothing you struggled with today".
4. **How many cards?** (**How many questions?** for Test): leave it empty for all.
5. **Options** (collapsed): **Practice** ("Study today’s cards without touching the schedule."), **Time limit** and **Rating** (Cards), **Only cards with** (a classification), **Query** and **Saved query**, **Postpone today's cards**, and **Question types** for Test.

`Enter` starts (**Start · 20 cards**), `Esc` closes. If a start is not possible the window stays open and says why; your open session is untouched. The app remembers the last mode for each deck and the last scope for each mode.

### A Cards session

Flip the card and say how well you knew it; that sets when it comes back.

**Answer buttons.** By default there are two: **Still learning** (`1`) and **I know it** (`2`). If you prefer four: **Again** (`1`), **Hard** (`2`), **Good** (`3`), **Easy** (`4`); each shows when the card comes back (`10m`, `3d` and so on). Switch in the session with **Show four answer buttons** / **Show two answer buttons**, or for good under **Settings** → **Studying and cards** → **Modes and reading** → **Answer buttons**.

**Keys: one key, one job.**

| Key | Action |
|---|---|
| `Space` | Flip the card (front ⇄ back), as often as you like. **Never rates.** |
| `1`–`2` (two buttons) · `1`–`4` (four buttons) | Rate; only once the back has been seen |
| `1`–`9` | On a multiple-choice question, pick that option |
| `Enter` | Presses only the focused button or the card, nothing else |
| `H` | Hint (on the question) |
| `S` | Set the card aside (until you bring it back) |
| `B` | Leave the card for tomorrow (not again today) |
| `Z` | Undo last answer |
| `Ctrl+1`–`Ctrl+7` | Toggle a flag |
| `N` | New cards from text |
| `F` | Focus mode |
| `Esc` | Pause the session |
| `R` (on the summary) | Study mistakes again |

A key held down counts as one press: holding `2` rates one card only. While you are in a text field (for example typing an answer) only `Esc` works. `Space`, the rating keys, `H`, `S`, `B`, `Z` and `Esc` can be changed on the **Shortcuts** page; `Enter`, the option digits, flags, `R`, `F` and `N` are fixed (**Fixed keys while studying**). The key hints on screen show the keys you assigned.

**Pausing and continuing.** `Esc` (or **Pause**) does not end the session: it shows what you have done so far ("Paused — you can continue where you left off") and keeps the session open. **Continue**, or **Continue** at the top of the launcher, returns to the same session.

**Hints.** `H` or **Hint**. If the note has an example sentence, the first hint is context: you see the word inside that sentence; when the word is what the card asks for, it is hidden as a gap ("Fill the gap in the sentence", for example `I ___ home.`). Further hints reveal letters of the answer (setting **Hint style**: first letters, start of every word, or some letters). Notes without an example sentence start with letters.

On the built-in Vocabulary and Subtitle cards the front shows only the word; the first hint brings the example sentence with the word highlighted. On the reverse (Production) card the first hint is the sentence with the term blanked out. The rule is simple: a hint shows context, never the answer.

**Waiting.** A card on a short learning step (say, back in 10 minutes) may be shown a few minutes early; this learn-ahead window is a setting (**Show learning cards early**, 20 minutes by default; 0 = always wait for the step). Changing it during a session never takes away the card on screen. When every remaining card really has to wait, the screen says "Next card in N min" and counts down: **Wait**, **Study something else** or **Finish**. The session stays open.

**When the session ends**, **Session complete** shows cards studied, accuracy, time and mistakes. **Study again** opens the launcher with **Struggled with today** selected; `R` studies the mistakes right away.

### Modes

| Mode | Schedule | What it does |
|---|---|---|
| **Cards** | Changes (not in practice scopes) | Flip the card, say how well you knew it. |
| **Learn** | Cards you get right move on (not in practice scopes) | Every card is asked as multiple choice, then written. A card is learned once you get it right at both steps. A wrong written answer goes back to multiple choice. |
| **Test** | Never changes | A mixed practice paper with a score at the end. |
| **Match** | Doesn't change | Match words with their meanings against the clock. |

All of them start from the launcher's **Mode**. In Cards mode you also have:

| Option | Where | What it does |
|---|---|---|
| Confidence | Launcher → **Options** → **Rating** → **Confidence 1–5** | Instead of buttons you answer "How sure were you?" from 1 to 5. |
| Timed | Launcher → **Options** → **Time limit** | A time limit for every card. |
| Focus | **Focus** button in the session or `F` | Full screen with distractions hidden. |

**Learn:** keys: `1`–`4` pick, `Enter` check / continue, `Z` undo, `Esc` end.

**Test:** **How many questions?** and **Options** → **Question types** (**Multiple choice**, **Written**, **True or false**; none picked means all). The paper opens with the focus in the first answer. The answers of a paper you leave half done are kept; come back with **Continue** in the launcher. Print the paper with **Print** or **Print answer key**. **Submit** scores it; "almost" counts as half a point. Submitting can be undone. A written answer is compared with the field the card asks for in its note type.

**Confidence rating:** the default mapping is 1–2 = Again, 3 = Hard, 4 = Good, 5 = Easy. Keys `1`–`5` work on the answer side. Change the mapping with the settings `confidence.map.1` … `confidence.map.5`.

**Timed mode:** enter the time in **Seconds**. For **When time runs out** choose **Count as Again** or **Skip**. The timer starts when the card appears, runs per card and cannot be restarted. An answer after the time is up is not accepted.

**Focus mode:** the settings `focus.fullscreen`, `focus.hide_counts` and `focus.auto` control full screen, hiding counters and starting every session in focus mode (**Settings** → **Studying and cards** → **Modes and reading**).

Practice sessions, Test and Match never change your cards' schedule. Each mode is its own module and can be turned off.

**Other study settings** (by setting key): `study.scheduler` (FSRS or SM-2), **Answer checking** (`study.answer_checker`: exact, ignore case and punctuation, allow typos, several answers), **Treat ı/i, ş/s… as equal** (`study.fold_turkish`), **Hint style** (`study.hint_strategy`), **Next day starts at** (`study.day_start_hour`, default 04:00; the "today" numbers follow it).

## Statistics and planning

**Statistics** → **Overview** opens with three questions:

- **How am I doing today?** What you answered today, how many were correct, time, streak and your goal, with a sentence comparing it to yesterday or the last 7 days ("Ahead of yesterday: 40 cards then, 52 already today.").
- **What's ahead?** Reviews in the next 7 or 30 days, with the busiest day marked. **Plan your pace** goes to Planning.
- **What am I struggling with?** Words you got wrong today, in the last 7 or in the last 30 days, most misses first (practice and tests included). **Study these** opens the launcher with those cards: it is a practice session, your schedule stays as it is. You can also pause a word from there or tag it "hard".

The detailed charts are below, each with a one-line meaning.

**Planning** → **Plan your pace**: answer **New cards a day?** with a number and optionally enter **My exam date (optional)**. The app shows a live forecast: "At this pace, in 4 weeks: ~85 cards and ~12 min a day.", the busiest day, when your new cards run out, and with an exam date how many new cards a day you need to start them all (**Use N**). Minutes use your average time per card over the last 30 days. **Apply** sets the new cards per day in the chosen deck's preset (creating one if the deck has none, and adding the exam date); it is undone in one step. Load balancing, the FSRS optimizer and workload by target retention are under **Advanced**.

**Goals**: a goal is a sentence with blanks to fill in (**Cards every day**, **Finish a deck**, **Minutes every day**…). Daily goals start from zero every day; goals with a deadline show a forecast at your current pace.

## Query language

The query language is the same in **Browse**, the study launcher's **Query** option, automation and MCP. Terms separated by spaces are combined with AND.

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
| `is:due` `is:suspended` (`is:aside`) `is:buried` (`is:tomorrow`) `is:flagged` | Due, set aside, left for tomorrow, flagged |
| `flag:red` · `flag:1` | Flag: 1–7, a colour name (EN/TR) or your own flag name; `flag:0` / `flag:none` unflagged, `flag:*` any |
| `stability>30` `difficulty>7` `lapses>=5` `reps<3` | Numbers; `=` `!=` `<` `<=` `>` `>=` |
| `retrievability<0.8` | Recall probability right now (0–1) |
| `added:7` `edited:7` `reviewed:1` | Added / edited / reviewed in the last N study days (days follow **Next day starts at**) |
| `due:3` | Due within N days |
| `level:B1` · `level>=B1` · `exam:TYT` | Classification (key or name); `level:*` = has a value |
| `word:app*` | Field search; `*` is a wildcard |
| `re:^ab` · `word:re:^ab` | Regular expression (all fields / one field) |
| `nid:` `cid:` | Note / card ids (comma-separated) |
| `-tag:easy` · `NOT tag:easy` | Not |
| `a OR b` · `(level:B1 OR level:B2)` | Or, grouping |
| `level!=B1` | Same as `-level:B1` (notes without a value match too) |

For a bad query the app shows where the error is. On **Browse**, name a query with **Save query**; saved queries can also be picked as **Saved query** in the study launcher (**Cards** mode → **Options**).

## Import and export

**Importing** now lives on the **Add cards** page: **Create** → **Add** → **From a file** (to paste a list, **Paste**). **Export, backup and printing** are under **Settings** → **Data** → **Export & backup**; its "Want to import cards?" link takes you to **Add cards › From a file**.

### Importing from a file

1. **Where the cards go**: pick the deck and note type.
2. Drop the file or press **Choose a file…**. Supported: CSV, TSV or text; Anki (.apkg, .colpkg); a Quizlet export; JSON; deck package (.fcpack).
3. **Map columns**: from each column's header choose what it becomes (a field role, **Tags**, a classification or **Don't import**). The app fills the mapping in from the header row, your last mapping or the column order, and shows the front and back live under **Your cards will look like this**. Tick **First row is a header** if there is one.
4. If the separators are wrong, press **Change**: **Between term and definition** (tab, comma, semicolon, dash, spaces or custom) and **Between cards** (**New line**, **Blank line** — for multi-line definitions — or custom).
5. **If the card already exists**: **Skip (recommended)**, **Update the existing one** or **Add anyway**.
6. If a classification column has unknown values, tick **Create missing classification options**.
7. **Import N cards**.

All of these lines are recognised:

```
dog, köpek
cat: kedi
Arda	porsuk
"Arda, Ali": porsuk
go	gitmek	I go to school by bus.
```

The file encoding is detected (UTF-8, UTF-16, Turkish Windows-1254). Anki's `#separator:` header line is read too. Auto-detection is not right for every file; you can always fix it by hand. To pick the format yourself, use **Advanced: choose the format yourself**.

### Quizlet

Paste Quizlet's export text into **Paste**, or choose its file. Default: tab between term and definition, new line between cards. If you chose custom separators in Quizlet, pick the same ones under the separators.

### Anki (.apkg, .colpkg)

- **Import:** choose the `.apkg` or `.colpkg` file under **From a file**. It shows how many notes, cards, decks and media files the package holds. With **Keep review history and scheduling** ticked, cards keep their Anki schedule (turn it off to start the cards fresh). A package brings its own decks and note types; if you picked a deck above, they all go under it. Cloze, image and audio cards are supported; old and new Anki package layouts are read.
- **Export:** **Export & backup** → **Export** → **Anki deck (.apkg)** or **Anki collection (.colpkg)**. **Include review history and scheduling** exports the schedule too. Packages are written in the older schema that every Anki since 2.0 imports. Exported packages are tested by opening them in real Anki (the official `anki` library, 26.09): notes, cards, scheduling, history, media and image occlusion masks arrive unchanged.

### JSON

A note format you can edit by hand:

```json
{ "format": "fc-json", "version": 1,
  "notes": [ { "deck": "English::A1", "notetype": "Classic",
               "fields": { "front": "dog", "back": "köpek" },
               "tags": ["animals"], "attributes": { "level": "A1" } } ] }
```

Without `notetype`, Classic is used. Fields and classifications are given by key or name; a role name (`term`, `meaning`, `example`…) works in place of a field too. To export, choose **JSON (.json)**.

### Deck package (.fcpack)

The least lossy way to move a deck to another Flashcard: notes, cards, media, classifications and (if you want) history travel together. The package keeps note ids and classifications exactly.

### Export and print

**Settings** → **Data** → **Export & backup** (**Deck actions** → **Export** on a deck page comes here too). Each tab starts with a line saying what it is for.

- **Export** writes the selected deck with its subdecks; with no deck selected, the whole collection. Formats: **Deck package (.fcpack)**, **Anki deck (.apkg)**, **Anki collection (.colpkg)**, **Text (.txt)**, **JSON (.json)**.
- On the **Print** tab choose a **Layout** (**Cards, 2 × 4 per page**, **Cards, 3 × 5 per page**, **List**), press **Prepare**, then **Print / save as PDF**. Backs are mirrored for double-sided printing.

Long jobs can be cancelled with **Cancel**; nothing changes when you cancel. A finished import is undone in one step (**Undo** on the result, or `Ctrl+Z`).

## Plugins

Plugins run in a sandbox (WASM) and can only do what you allow. They can add card types, study modes, voice providers, or hooks that fill fields when a note is added.

**Installing:**

1. **Settings** → **Advanced** → **Plugins** → **Install from file…** → choose a `.fcplugin` file.
2. The dialog shows the plugin's name, version, what it adds and the permissions it asks for.
3. Untick what you do not want to allow. Items marked "asks separately" are sensitive permissions.
4. Press **Install**.

Each plugin has **Turn on** / **Turn off**, **Permissions**, **Settings**, **Log** and **Uninstall**. When uninstalling you can tick **Also delete its saved data**; notes it created stay.

A plugin that hangs or crashes does not take the app down: calls are cut off by time, work and memory limits (the page shows them). A plugin that fails several times in a row is switched off ("Switched off after errors"); turn it back on with **Turn on**.

There is no plugin store yet; you install plugins from files.

## AI

AI is entirely optional: it stays off and nothing is sent until you set up a provider. Then you can make cards from a text, fill empty fields while you write, and ask for an explanation or a hint while you study.

**Where.** Two places, two jobs: **Create** → **AI** is only **Make cards with AI** (the deck is a field of the form; the **Generate with AI** button on the Create page leads there too). Provider setup and usage live in **Settings** → **Connections** → **AI**, on the **Providers** and **Usage** tabs. With no provider yet, the make-cards page takes you there with **Connect a provider**.

**Add a provider.** **Providers** → **Add provider** opens a gallery: **Claude**, **OpenAI**, **OpenRouter**, **Google Gemini**, **Groq**, **Mistral**, **DeepSeek**, **Ollama**, **LM Studio** and **Custom (OpenAI-compatible)**. Each has a short description and a "free option" or "local · free" tag. Pick one and three steps follow:

1. **Paste the key**: the "Get a key here:" link shows the provider's key page (for OpenRouter, `openrouter.ai/keys`). The key is stored only in the operating system's keychain, never in the database or a backup, and the app never shows it again. For Ollama and LM Studio this step is **Connect the server** and no key is needed. For **Custom** you enter the address and (if needed) a key; servers on your computer or home network do not need one.
2. **Pick a model**: the list comes from the provider; search it or type a model name.
3. **Test the connection**: a very short request (a few tokens) is sent to the model and you see the result ("The connection works." · "… answered · 420 ms").

If the test fails, the reason is in plain words, along with the provider's own message (never the key): "The key was rejected…", "No credit or quota left on the account…", "This model was not found or this key may not use it…", "Too many requests right now…", "Could not reach the server…", "Could not make a secure connection (certificate)…".

In the provider list every row shows its status (**ready**, **needs a key**, **no model chosen**, **test failed**) and has **Test connection** right in the row. The address is under **Advanced**; if it changes, you are asked before a stored key is sent to the new address. Keys belong to the profile: each profile has its own. The **Default provider** is used for every feature; with **Model per feature** you can, for example, use a strong model for making cards and a fast one for checks.

**Make cards:**

1. Choose a deck.
2. **Source from**: **Text**, **PDF** (with a page range) or **Web page**.
3. Set **Note type**, **How many** and optionally **Extra instructions** (for example "focus on verbs").
4. **Preview**. The **Before sending** dialog shows provider, model, estimated tokens and cost. **Show exactly what will be sent** shows the full text.
5. **Send**. Take the suggestions you want with **Add**.

**Generate with AI** in the **Bulk** grid leads here too. If the deck has a level classification, the cards are made for that level.

**While studying:** the study screen shows **AI hint**, **Explain** and **Check with AI** buttons.

**Cost:** the **Usage** tab shows requests this month and in the last 7 days. Set a monthly budget with the `ai.monthly_budget_usd` setting. Costs are estimates from the price table; keep the **Prices** table current. With Ollama and LM Studio the model runs on your computer and nothing leaves it.

## MCP

MCP lets assistants such as Claude Desktop and Claude Code work with your collection. The **Settings** → **Advanced** → **Assistants (MCP)** page has two tabs: **Connect your assistant** and **External MCP servers**.

**Connect your assistant:**

1. **Which assistant do you use?**: **Claude Desktop**, **Claude Code** or **Other MCP client** (Cursor, VS Code…).
2. **How to set it up** gives three steps and a ready text to copy. The text holds this profile's real `collection.db` path and the path of the `fc` program. **Copy**, then paste it into your assistant (Claude Desktop: **Settings → Developer → Edit Config**; Claude Code: run the command in a terminal). Restart the assistant.
3. **How to test it**: ask your assistant to "List my decks". If it names your decks, the connection works.

The server runs through the `fc` command-line tool. The installer does not include `fc`: build it from the source code with `cargo build -p fc-cli --release` (it lands in `target\release\fc.exe`) or get it separately. If the app cannot find `fc` next to itself or on your PATH, it says "The fc program was not found"; type its full path in the box there and the setting text updates itself.

To set it up by hand (replace `C:\Tools\fc.exe` and the collection path with your own):

```bash
claude mcp add flashcard -- "C:\Tools\fc.exe" --db "C:\Users\<you>\AppData\Roaming\app.flashcard.desktop\collection.db" mcp serve
```

Claude Desktop (`%APPDATA%\Claude\claude_desktop_config.json`; write `\` twice inside JSON):

```json
{ "mcpServers": { "flashcard": {
    "command": "C:\\Tools\\fc.exe",
    "args": ["--db", "C:\\Users\\<you>\\AppData\\Roaming\\app.flashcard.desktop\\collection.db", "mcp", "serve"] } } }
```

That path is the main profile's collection. For another profile, `--db` must point at that profile's file, `...\app.flashcard.desktop\profiles\<id>\collection.db` (see [Data folder](#data-folder)); the ready text on the page already uses the open profile. `fc` then uses that profile's AI keys. The app lock does not apply to `fc` or the MCP server.

**What the assistant can do.** **It can read**: **List decks**, **List note types**, **List classifications of a deck**, **Search cards**, **Read a note**. **It can change**: **Create a deck**, **Add notes**, **Edit a note**, **Set a classification (asks for confirmation)**. No tool deletes anything.

- Every change an assistant makes is an ordinary step in the app; `Ctrl+Z` undoes it.
- With **Read only** on, the assistant can see your decks but cannot change anything: the tools that change things are not offered to it at all. `mcp serve --read-only` does the same for one connection.
- The **External MCP servers** tab works the other way round: **Add server** adds other servers whose tools the app's AI features may use. The app shows the program it will start and asks you to confirm.

## Browser extension

For Chromium-based browsers such as Chrome, Edge and Brave. It sends a word or sentence you select on a page to the app's **Inbox**.

**Setup:**

1. Install the extension. Load the unpacked build via `chrome://extensions` → **Developer mode** → **Load unpacked** and choose the extension folder.
2. In the app, turn on the **Local API** module (off by default): `Ctrl+K` → **Turn a module on/off** → `{"id": "local_api", "enabled": true}`. **Local API** appears under **Settings** → **Advanced**. The default port is `28735`.
3. Extension button → **Connect**. The app opens a **Connection request** dialog; allow it.

**Use:**

1. Select text on a page → right-click → **Add to deck** (or `Alt+Shift+A`, or the toolbar button).
2. Fill in **Front**, **Back**, **Deck** and the deck's **Classifications** (for example Level).
3. **Add to inbox**.
4. In the app: **Inbox** → edit the suggestion → **Accept** or **Reject**. Nothing becomes a card until you accept it. A proposal without a deck asks for one first.

**The captured sentence is kept.** Each row says which card type it becomes. If an item came with a sentence, the sentence goes into the note's example sentence field and the page into its source field; with no note type chosen, the item becomes a **Subtitle card**, which can hold the sentence ("Card type: Subtitle card (it came with a sentence, which stays on the card)"). When you edit before accepting, the sentence and the other fields are kept too.

Items added while the app is closed wait in the extension and are sent when the app opens. The extension only connects to `127.0.0.1`. If you changed the port, enter the same value in **Local API port** in the extension's settings. You can see and revoke connected programs on the **Local API** screen.

**Proposals per program per day** (`local_api.inbox_daily_max`, default 200): the most cards one program may propose in 24 hours. Change it with `Ctrl+K` → **Change a setting** → `{"scope": {"kind": "app"}, "key": "local_api.inbox_daily_max", "value": 100}`. An automation rule that accepts inbox proposals without your review is marked with a warning on the **Automation** screen: whatever a paired program or a plugin proposes then becomes a note right away.

The inbox also receives suggestions from `flashcard://` links and the `Ctrl+Alt+N` quick-add window.

### Subtitle (video subtitles)

The **Subtitle** browser extension turns a word you add with **+** in a video's subtitles straight into a card in a deck (not the inbox): the word and its meaning, the line it came from (the word in bold), its translation, a screenshot, the audio clip and a link to that moment of the video.

1. Turn on the **Subtitle** module in the app (off by default): `Ctrl+K` → **Turn a module on/off** → `{"id": "subtitle", "enabled": true}`. **Subtitle** appears under **Settings → Connections**.
2. On the **Subtitle** screen type a name, press **Create key** and copy the key. It is shown **only once**.
3. In the extension pick **Ayarlar → Öğrenme servisi → Subtitle Kartları** (Settings → Learning service); paste the address from the screen (`http://127.0.0.1:28735/subtitle`) as the server address and the key as the access key, then press **Bağlantıyı test et** (Test connection).

Words go to the deck picked in the extension, else the screen's **Deck**, else a **Subtitle** deck made with the first word. The same word from the same line twice is one card; from another line it is a new card. Each word is one undo step. While the app is closed the extension keeps words in its queue and sends them once the app is open. **Revoke** makes a key stop working at once.

## Automation

**Settings** → **Advanced** → **Automation**: let the app do repetitive jobs. **Start from a ready rule** offers six recipes, each with a live count of the cards it matches right now:

- **Tag cards I keep forgetting** (a "difficult" tag on the 4th lapse)
- **Set aside cards that never stick** (after 8 lapses)
- **Move learned cards to an archive deck** (cards you will remember for 90+ days)
- **Tag new Subtitle cards**
- **Set aside red-flagged cards**
- **Remove “difficult” once a card is easy** (once a day)

A recipe opens filled in; you can change the tag, the deck or the number, and nothing happens until you save. Rules read as one editable sentence; the raw editor is under **Advanced**. The run history is a readable list.

## Profiles and app lock

Both are on the **Settings** → **General** → **Profiles & lock** screen (the profile menu also has **Switch profile** and **Lock now**).

**Profiles.** Each profile is a separate collection with its own decks, media, settings and AI keys, for example one per person on a shared computer.

- **Add profile** creates one (type it in **New profile name**).
- **Open** switches to a profile: "Opening the profile — the app restarts…". The app restarts on that profile's collection and keeps opening it until you switch again.
- **Rename** changes the name.
- **Delete** removes the profile for good: its folder (collection, media, backups, logs, crash reports) and its keychain entries (AI keys, lock hash). A locked profile asks for its PIN/password first. The **Main profile** and the profile **In use** cannot be deleted.

**App lock.** Each profile can have its own lock:

1. Under **New PIN/password** choose **PIN (digits)** (4–12 digits) or **Password** (6–128 characters) and press **Set lock**.
2. **Lock after inactivity**: **Never**, 1, 5, 15 (default), 30 or 60 min.
3. **Lock now** (or **Lock** in the tray menu) locks right away. **Change** and **Remove lock** need the **Current PIN/password**.

When the app starts with a locked profile, or locks while open, it shows **Flashcard is locked**. Enter the PIN/password and press **Unlock**, or choose **Open another profile**. After 5 wrong attempts you have to wait before the next one, starting at 1 second and doubling up to 5 minutes; the count survives a restart.

The PIN/password itself is never stored, only a salted Argon2id hash: in the system keychain, or in the profile list file (`profiles.json`) where the keychain does not keep entries across restarts. The screen tells which.

While the app is locked, the Local API, automation rules, plugins, the clipboard watcher and media files are closed. `flashcard://` links that arrive are not handled; the lock screen lists them under **Links waiting for unlock**. They run after you unlock, unless you press **Don't open them**.

**What the lock protects against.** The lock keeps out casual access to this window only. The collection file is not encrypted: anyone who can use your computer account can copy it. The `fc` command line and the MCP server are not locked. If the lock's hash is removed (for example the keychain entry is deleted), the app stays locked and shows **The lock's data is missing**; anyone at the keyboard can then tick "I understand: remove this profile's lock" and press **Remove the lock**. So the lock does not protect against someone with access to your operating system account.

## Reminders and tray

On **Settings** → **General** → **Reminders**:

- **Remind me every day** turns on one OS notification a day ("35 cards are waiting for you today (20 due, 15 new)."), off by default. The number is what **Start studying** would really serve: it follows the daily new-card and review limits instead of counting every card you have not started.
- **Time** (default 19:00) and **Days** (default every day).
- There is no reminder on a day you already studied (the day follows **Next day starts at**) or when no cards are waiting. If the app was closed or asleep at that time, the reminder still comes up to 3 hours later.
- **Send a test notification** shows one now.
- **Closing the window keeps the app in the tray**: closing the window only hides it, so reminders keep coming. Reminders come only while the app runs (window or tray).

The tray icon menu: **Open Flashcard**, **Lock** (locks the app if a lock is set), **Quit** (really quits). Clicking the icon also brings the window back.

## Crash recovery and logs

**Recovery.** The app writes a run marker (`run.json`) at start and removes it on a normal exit. If it is still there at the next start (crash, killed process, power loss), the app checks the database (setting `maintenance.check_after_crash`, on by default) and shows **Flashcard did not close normally** (or **Flashcard closed unexpectedly** after a crash). It lists what can be picked up: the database check result, an unfinished study session (**Resume studying**), unsaved forms (restored when you open them again) and the crash report. **Open maintenance** goes to **Logs & updates**, **Continue** closes the dialog. Your collection is saved after every change, so nothing else is lost.

**Logs & updates** screen (**Settings** → **Advanced**): version, whether the last run closed normally, updates, **Log folder** and **Data folder**, **Check the database**, crash reports and **Delete crash reports**.

**Logs.** The app keeps a local log, `logs\flashcard.log`. It starts a new file at 1024 KiB and keeps 4 older files (settings `maintenance.log_max_kb`, `maintenance.log_files`; detail with `maintenance.log_level`). Crash reports go to the `crash` folder. Secrets are removed from every line before it is written.

**Bug report:** **Logs & updates** → **Export logs…** → choose where to save. The `.zip` holds the logs, crash reports and a short system summary. Everything is redacted again first: API keys (including the exact keys the app holds), tokens, passwords, email addresses, the user name in home-folder paths (`C:\Users\<user>`) and the lock hash. The message says how many secrets were removed. Nothing is sent anywhere; attach the file to your report yourself.

The run marker, logs and crash reports belong to the profile: they are in the active profile's folder (see [Data folder](#data-folder)).

## Data folder

On Windows the app data folder is `%APPDATA%\app.flashcard.desktop\` (that is `C:\Users\<you>\AppData\Roaming\app.flashcard.desktop\`). **Logs & updates** shows the active profile's **Data folder** and **Log folder**.

| What | Main profile | Other profiles |
|---|---|---|
| Folder | `%APPDATA%\app.flashcard.desktop\` | `%APPDATA%\app.flashcard.desktop\profiles\<id>\` |
| Collection | `collection.db` | `collection.db` |
| Media, safety backups | `media\`, `backups\` | `media\`, `backups\` |
| Logs, crash reports, run marker | `logs\`, `crash\`, `run.json` | `logs\`, `crash\`, `run.json` |

The profile list is `profiles.json` in the app data folder. `<id>` is a long generated id; open the profile and look at **Data folder** to find it. AI keys and the lock hash are not in these folders but in the system keychain.

## Backup and restore

**Settings** → **Data** → **Export & backup** → **Backup**:

- **Create backup…** writes the whole collection to one `.fcbackup` file: notes, cards, history, classifications, settings and media.
- **Restore backup…** asks for a backup, shows its date and note count and asks you to confirm. The current collection is backed up first, then replaced. The message shows where that safety backup went (the `backups` folder in the collection folder).

Good to know:

- After a restore, undo history starts over; earlier steps cannot be undone.
- A backup from a newer app version cannot be restored into an older one.
- Programs connected to the Local API lose their permission on restore and must connect again.
- Backup and restore work on the profile in use. A backup does not contain AI keys or the app lock.
- On Windows the main profile's collection file is `%APPDATA%\app.flashcard.desktop\collection.db`, another profile's is `%APPDATA%\app.flashcard.desktop\profiles\<id>\collection.db`. Do not copy it by hand while the app is open; use **Create backup…**.

## Troubleshooting

**Columns came out wrong on import.**
In the mapping step, pick the right field from each column's header; if the separators are wrong, press **Change** and fix **Between term and definition** (for example **Tab**). If you already imported, use **Undo** on the result or `Ctrl+Z`.

**The example sentences of my imported list are gone.**
If the chosen note type has no example sentence field, the sentence column goes nowhere; the app says so in the mapping step. Switch to a type with an example sentence field such as **Vocabulary**, or give a field of your own type the **An example sentence** role (**Note types** → **Contains:**).

**Import is not in Settings any more.**
It moved in v1.3: **Create** → **Add** → **From a file**. Export and backup are under **Settings** → **Data** → **Export & backup**.

**I pressed Space but the card was not rated.**
Right: `Space` only flips the card. To rate, press `1`–`2` (`1`–`4` with four buttons) or a button after you have seen the back.

**I pressed `Esc`. Is my session gone?**
No, you paused. **Continue** at the top of the launcher returns to the same session.

**I deleted something by mistake.**
Undo with `Ctrl+Z`. If a deck or note went to the trash, use **Settings** → **Advanced** → **Maintenance** → **Trash** → **Restore**.

**It says "Undo stops here".**
The collection was replaced from a backup or the trash was emptied for good. Earlier history cannot be undone.

**"Nothing to study right now." / Today's cards is empty.**
The launcher says why under the scope: nothing is due, the daily limit is reached, or the next card comes in a few minutes. To study anyway, pick **Studied today**, **Struggled with today** or **All cards** (practice, schedule unchanged), or type a query (`deck:English`) in **Options** → **Query**. Change the daily limit under **Planning** → **Plan your pace** or **Deck options**.

**The browser extension says the app is not reachable.**
Check that the app is open, the **Local API** module is on and the port matches the extension's. The item is not lost; it waits in the extension.

**Claude cannot change my collection.**
Turn off **Read only** on the **Assistants (MCP)** screen and make sure you are not using `--read-only`. The server does not open a collection with an older schema: open the app once first.

**AI says "This provider needs an API key".**
Under **AI** → **Providers**, save the key in the provider's row and press **Test connection**; the result says in plain words what is wrong. If you see "No system keychain", keys cannot be stored on this system.

**A plugin shows "Switched off after errors".**
Check its **Log** and **Last error**, then try **Turn on** again. If it keeps failing, uninstall it.

**A menu is missing.**
Its module may be off. Check with `Ctrl+K` → **List modules** and turn it on with **Turn a module on/off**. If you don't know where a setting is, type in **Search settings**.

**I think the database has a problem.**
**Settings** → **Advanced** → **Maintenance** → **Database check** → **Check**. If it finds problems, press **Repair**; the repair can be undone too.

**Windows says "Windows protected your PC".**
That is SmartScreen; the installer is not code-signed. Click **More info** → **Run anyway**. Download the installer only from the project's releases page.

**The app says it is locked and asks to remove the lock.**
The screen **The lock's data is missing** means the profile is locked but its PIN/password hash is gone (for example the keychain entry was removed). The app stays locked instead of opening unchecked. Tick "I understand: remove this profile's lock", press **Remove the lock**, then set a new lock on **Profiles & lock**. Your cards are not affected.

**The app restarted after I switched profile.**
That is expected: **Open** on another profile restarts the app on that profile's collection. Switch back the same way.

**Where are my logs? How do I send a bug report?**
**Logs & updates** shows the **Log folder** (`logs` in the active profile's folder, see [Data folder](#data-folder)). Press **Export logs…**, save the `.zip` and attach it to your report. Secrets are removed and nothing is sent automatically.
