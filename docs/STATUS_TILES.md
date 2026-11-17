# Status tiles: a tile that shrinks to a status icon (US5.4)

Design of User Sprint 5 item US5.4 (#752), written by thread D in Phase 2 before its build. The wish: the warning and notice tiles and the *Update all accounts* tile can be made as small as **1 × 1** cell. At that size there is no room for text, so the tile **turns into a status icon** that tells the state at a glance, the way a desktop widget in a panel shrinks to an icon and still shows its status.

This design defines **one shared state set** that any tile can adopt, how each state looks, sounds to a screen reader and explains itself, when a tile switches to the icon, and what a click on the icon does. The first build applies it to four tiles (§5).

Release stage: **Alpha** (D-274). Nothing new is stored: the dashboard document keeps the same frame keys, only a smaller frame becomes allowed. **The book format is not touched**, it is not a **[format]** item, and no human gate applies (no bank, keychain or AI code changes; the bank tile only reads what it already reads). An older build refuses a dashboard that holds such a small frame (`frame-too-small`, as after #521); in Alpha this is accepted, the dashboard itself is not changed or lost, and opening it again in this build shows it.

**What stays true (D-342):** the start screen is complete without it. No built-in Overview or template uses the icon size; a tile shows as an icon only when the user makes it that small. **Numbers come from code (AGENTS.md §8, D-065):** the state is decided in Rust from the figures the tile already has; QML only draws it. **Nothing leaves the app:** no system notification, no sound, no network (§3.1 rule 4).

## 1. The shared state set (D-734)

Every status tile is in exactly one of five states. The state is never shown by colour alone (AGENTS.md §6): each state has its **own shape**, filled or hollow, **its own mark** inside it, a colour, and words.

| State (word in code) | Means | Shape and mark | Colour (Kirigami theme) | Short words (screen reader, beside the icon) |
|---|---|---|---|---|
| `fine` | nothing to do | filled **circle** with a **tick** | `positiveTextColor` (green) | *All fine* |
| `to-do` | something waits for the user | filled **triangle** with **!** | `neutralTextColor` (orange) | tile's own, e.g. *2 warnings apply* |
| `running` | work is under way right now | **ring** with a gap that turns slowly | `highlightColor` (blue) | *Updating…* |
| `failed` | the last attempt did not work | filled **diamond** (a square on its point) with **×** | `negativeTextColor` (red) | tile's own, e.g. *Last update failed* |
| `unknown` | the state can't be told, or nothing is set up yet | **hollow circle** with **?** | `disabledTextColor` (grey) | tile's own, e.g. *No warnings set yet* |

- **Why orange for "something to do" and red only for "failed":** the maintainer's example in #752 was red for "something to do". With five states, red is kept for "it did not work", so a triggered warning and a broken bank update look different. The triangle with "!" still reads as "look here". This is the safe default; a later round may change the colours without touching the shapes.
- The shapes and marks are **drawn by the app** (Qt Quick shapes and one text glyph), not taken from the icon theme, so they look the same on Plasma, GNOME, Windows and macOS and never vanish without an icon theme. Filled versus hollow and the different outlines keep the states apart in greyscale and for colour-blind users.
- The **running** ring turns once every two seconds and stops turning in the other states; it never blinks.
- The state word is a fixed English word from Rust (`fine`, `to-do`, `running`, `failed`, `unknown`), like the alarm states (D-721). The words the user sees are translated in QML (KI18n), never sent from Rust as text (D-034).
- **Order when several things apply:** `running` > `failed` > `to-do` > `unknown` > `fine`. A tile is only "fine" when nothing else applies, so an unchecked rule never reads as fine (as D-722 does for the full tile).

**Where it lives.** A small Rust module `kassensturz-report/src/status.rs` holds the `TileStatus` enum, its `id()` word and the order above, plus one pure function per tile (§5) that takes the figures the tile already uses and returns its status and a count. The bridge puts the word and the count into the objects the tiles already read (§5). One QML component, `StatusIcon.qml`, draws any status: shape, mark, colour, short words, hover text and the screen-reader label. A tile adopts the icon by handing it a status word, a count and its own words; nothing else is shared.

### Each tile's own emblem (D-750)

At 1 × 1 the state alone doesn't say which tile it is, so a tile that has an emblem draws it as the icon and shows its state as a **badge** in the lower right corner, half the icon's size, with the same shape, mark and colour as above (#787, mock-up approved by the maintainer in US5 Phase 3).

| Tile | Emblem |
|---|---|
| Warnings (`notice.alarms`) | a bell |
| Transactions without a category (`notice.unsorted`) | a tag |
| Warnings about old bank data (`notice.fetch_gap`) | a bank with a clock |
| Update all accounts (`bank.update_all`) | two arrows turning in a circle |
| Left this month (`forecast.month`) | a calendar page with a line taken off (US7) |
| Coming up (`plan.coming_up`) | a calendar page with an arrow ahead (US7) |

- The emblem is drawn by the app from 24 × 24 outlines (like the state shapes), in the theme's **text colour**, so it is dark in a light theme and light in a dark one. The badge's mark uses the theme's background colour, as before.
- When the state is `to-do` or `failed`, the icon also gets a **frame** tinted in the state's colour, so it stands out from the side of the eye. `fine`, `running` and `unknown` have no frame.
- A tile type with no emblem (none yet) shows the state shape filling the icon, as in the first build. `StatusIcon` takes the tile type as `emblem`.

## 2. Words: screen reader and hover text

- **Screen-reader name** (`Accessible.name`): the tile's name, a colon and the short words, e.g. *Warnings: 2 warnings apply*, *Update all accounts: last update failed*. The role is a button (§4).
- **Hover text** (a tooltip after the usual delay, and `Accessible.description`): one plain sentence that says what the state means and what to do, then *Click to see the whole tile.* Examples: *All fine. None of your 3 warnings applies today. Click to see the whole tile.* · *2 transactions have no category yet. Click to see the whole tile.* · *The last update didn't work for 1 account. Click to see the whole tile and try again.*
- The sentences reuse the full tile's own wording where one exists (the alarm sentences of D-722, the unsorted count of D-637), so a user reads the same words small and large.
- Plural forms go through `i18ncp`. New strings get German entries in `po/de` in the build (AGENTS.md §9.4).

## 3. When a tile turns into an icon (D-735)

The switch is decided **by cell count**, never by pixels, so it doesn't change when the window is resized.

- Each status tile type gets two sizes in `grid.rs`: its **smallest size**, which becomes **1 × 1**, and a new **full size**, the smallest frame its text form stays readable in. The full size is the type's smallest size before this item (table in §5).
- A tile whose frame is **narrower or lower than its full size** shows the **icon form**; otherwise the **full form** as today. The rule is a Rust function, `ComponentType::shows_as_icon(frame)`; the layout bridge adds the answer as `compact` to each component's object, and QML only reads it.
- **Icon form:** the icon sits in the middle of the frame, as large as the frame's shorter side allows, capped at 3 grid units. When the frame is **at least 3 columns wide** (for example a 3 × 1 Warnings tile) the short words stand next to the icon on one line, elided if needed. The heading and the grey filter lines are not shown in the icon form.
- **One-column view** (a narrow window, D-287): every frame is full width there, so a tile in icon form shows the icon with its short words beside it. Whether it is in icon form still follows its cell count, so the layout reads the same in both views.
- **Edit mode:** the icon form shows as it will on the page, and a resize handle may go down to 1 × 1. Selecting, moving and the Properties panel work as for any tile. The status tiles' *Add* default sizes stay unchanged (they are added full size).
- Other tile types keep their smallest sizes. A type adopts the icon later by getting a full size and a status function; nothing else changes.

## 4. A click opens the whole tile (D-736)

- On the page (not in edit mode), the icon is a **button**: a click, a tap, or **Enter** or **Space** when it has keyboard focus opens a **window over the page** (a Kirigami dialog) with the tile's name as its title and the **full form** of the same tile inside, at the type's default width and height. Everything in it works as in a large tile: the warning lines open the Budgets page, the unsorted rows open their register, *Get new transactions* and *Update all accounts* start an update.
- The window closes with its *Close* button, **Esc**, or a click outside; focus goes back to the icon. When an action inside the window opens another page (the Budgets page, a register), the window closes first.
- The icon is in the tab order where its tile is in reading order (D-287), like any other tile.
- In **edit mode** a click selects the tile, as for every tile; the window doesn't open, so arranging never starts an update.
- The window shows the live state: while an update runs the icon behind it turns and the full form inside shows the progress.

## 5. The first build: four tiles

| Tile | Type | Full size (cols × rows) | Its status comes from | `fine` | `to-do` | `running` | `failed` | `unknown` |
|---|---|---|---|---|---|---|---|---|
| Warnings | `notice.alarms` | 3 × 2 | the alarm view of the tile's rules (`BookAlarms`, D-721): `status` and `count` added to its view object | rules set, none triggered | a rule triggered, or its budget or account is gone (count = those lines) | — | the rules could not be read (the view's `error`) | no rule set yet, or a rule could not be compared (D-722 point 5) |
| Transactions without a category | `notice.unsorted` | 4 × 1 | the page view's unsorted count (D-637): `unsortedStatus` added to the page view | count 0 | count > 0 (count = how many) | — | — | the page figures are not there yet |
| Warnings about old bank data | `notice.fetch_gap` | 4 × 1 | the page view's fetch gaps (D-328): `fetchGapStatus` added | no gap | a gap (count = how many accounts) | — | — | the page figures are not there yet |
| Update all accounts | `bank.update_all` | 3 × 1 | `updateAllState()` and the update summary of the bank object (D-497, #521): `status` and `count` added to both | the last update worked for every account, or no update ran in this session and a last update date is known | the last update brought notices to check (`toCheck` > 0) | `bank.updating` is true | the last update failed, as a whole or for some accounts (count = failed accounts) | bank connections can't be used here, no account is linked yet, or a linked account was never updated and no update ran in this session |

- *Notices* in the US5 sorting list means the **Transactions without a category** tile (#535). *Warnings about old bank data* adopts the icon in the same build because it is a warning tile of the same kind and costs one status function. The *Note about moved money* (`notice.transfers`) is a plain explanation with nothing to do, so it keeps its text form.
- **US7 added two more** (D-769, D-770, `docs/SCHEDULES.md` 12): both are filled by the app's `PlannedPayments` object, whose view carries `forecast.status`/`forecast.statusCount` and `comingUpStatus`/`comingUpCount`; the view's error object shows as `failed`.

| Tile | Type | Full size (cols × rows) | Its status comes from | `fine` | `to-do` | `running` | `failed` | `unknown` |
|---|---|---|---|---|---|---|---|---|
| Left this month | `forecast.month` | 3 × 2 | `status::forecast` over the forecast of the open checking and cash accounts | a plan counts and nothing passed unmatched | a planned payment passed without an entry and isn't counted (count = how many) | — | the planned payments could not be read | no plan counts, or no open checking or cash account |
| Coming up | `plan.coming_up` | 3 × 2 | `status::coming_up` over the plans and the payments of the next 30 days | a plan exists (count = payments coming up, 0 says "Nothing coming up") | — (a payment that passed waits on *Left this month*, not counted twice) | — | the planned payments could not be read | no plan yet |

- `running` is the only state QML reads directly (`bank.updating` changes while an update runs); the bridge still returns the full status word for the other cases, so the order of §1 is applied in one place: `StatusIcon` shows `running` whenever the bank object says so and the Rust word otherwise.
- **Why "unknown" for "no rule set yet" and "no account linked":** grey with "?" says "nothing to report, nothing set up" without claiming "all fine". The hover text says what to do (*No warnings set yet. Click to add one in this tile's settings.*; *No account is linked to a bank yet.*).

## 6. Tests

- `status.rs`: one unit test per tile and state, from the real inputs (an `AlarmsView` from `alarms.rs`, a `Summary` of the fake bank backend, D-055), including the order of §1 when several apply.
- `grid.rs`: each of the four types accepts 1 × 1 and still refuses 0 × 0; `shows_as_icon` is true just below the full size in either direction and false at it; other types keep their smallest sizes (`frame-too-small` unchanged for them).
- **Read back the way QML does** (AGENTS.md §9.4): the layout object carries `compact`; the alarm view, the page view and both bank objects carry the new `status` (and `count`) fields, each read from the `QVariant` map in a bridge test.
- A dashboard with a 1 × 1 tile of each kind round-trips through *Save to a file…* and *Add from a file…* unchanged (`docs/DASHBOARD_FILE.md`).
- The panel smoke test (`tools/check-panel.sh`) loads `StatusIcon.qml`, and checks that a tile with an emblem draws it with the state as a half-size badge and a frame for `to-do`, and that a type without one draws the state alone (D-750).

## 7. Decisions

| Number | Decision |
|---|---|
| D-734 | One shared status set for tiles: `fine`, `to-do`, `running`, `failed`, `unknown`, each with its own drawn shape and mark as well as a colour (never colour alone), short words for the screen reader and a hover sentence. Decided in Rust (`status.rs`), drawn by one QML component (`StatusIcon.qml`). `running` > `failed` > `to-do` > `unknown` > `fine`. Orange for "something to do", red only for "failed" (safe default). |
| D-735 | A status tile type has a full size (its old smallest size) and a smallest size of 1 × 1; below its full size in columns or rows it shows the icon form. Decided by cell count in Rust (`shows_as_icon`, `compact` on the layout object), never by pixels. Short words beside the icon from 3 columns and in the one-column view. |
| D-750 | Each status tile shows its own drawn emblem in the theme's text colour (bell, tag, bank with clock, turning arrows), with its state as a corner badge half its size, and a frame in the state's colour for `to-do` and `failed` (#787). |
| D-736 | On the page, a click, tap, Enter or Space on a status icon opens the full tile in a window over the page; Esc, Close or a click outside closes it and focus returns to the icon. In edit mode a click selects as for every tile. First build: Warnings, Transactions without a category, Warnings about old bank data, Update all accounts. |
