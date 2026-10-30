# The report designer, part 2: grid editor, groups and formulas (M6b)

Written in M6b.0 (Sprint M6b, Phase 0). It holds what the M6b steps build on: the grid rules and the editor's undo, the dashboard document v1, category groups and the hide list, the formula language, editing AI-prepared changes (#191) and the capability check in "Test connection". It continues `docs/REPORT_DESIGNER.md` (M6.0), whose rules still hold: numbers come from Rust as text (D-065), aggregation is exact and stays in `kassensturz-report` (D-277), documents are untrusted input with fixed limits (D-281). Decisions are D-286 to D-294 in `docs/DECISIONS.md`; they are **Proposed** until the maintainer approves them. Release stage: **Alpha** (no migrations; the book format may change; the dashboard document keeps its version field).

The maintainer's answers this design builds on, word for word where he wrote them:

- **Q-221** (groups storage), 2026-10-29: "The Groups created or used for statistics, should be kept of the transaction informations". Read as: in the book, so they travel with it and its backups, but **apart from** the transactions and the category tree, in report-definition tables of their own (section 3).
- **Q-222 = A** (formula syntax): LibreOffice style, the function list in the planning file, wave 1 first. "SUM was only 1 example, there a way more things that could be used."
- **Q-223 = A**: the capability check in "Test connection" is part of M6b, as M6b.9.

## 1. The grid (D-287)

The Overview's components sit on a grid the user arranges. The page parts that are not components (the accounts total, the fetch-gap warnings, the month switcher, the transfers note) stay fixed above the grid; only the grid is edited.

**Geometry**

- **12 columns** of equal width (12 divides into halves, thirds, quarters and sixths). A column is the page width less the gaps, divided by 12.
- **Rows** are a fixed height that grows with the user's font size: one row is `Kirigami.Units.gridUnit * 4`. Nothing is a fixed number of pixels.
- A component's `frame` is `col` (0 to 11), `row` (0 or more), `cols` (1 to 12) and `rows` (1 to 24), with `col + cols ≤ 12`.
- **Smallest sizes** (columns × rows), so a component never becomes unreadable: chart 3 × 3, key figure card 2 × 2, progress bar 3 × 1, table 4 × 3. A resize stops there.
- **Reading order** is by row, then by column (top left first). Tab order, the screen-reader order and the one-column order of a narrow window all follow it.
- **No overlaps**, ever: a stored dashboard has none, and every edit keeps it so.

**Edits**

- **Snapping:** a drag ends on whole cells. The drop place is the cell under the component's top-left corner, rounded to the nearest cell; a resize rounds to whole cells. Both are clamped to the grid and to the smallest size.
- **Push down, never sideways:** a component that lands on others pushes each of them down just far enough, and those push the ones below them, in reading order, until nothing overlaps. Nothing moves left, right or up by itself.
- **No automatic tidying:** gaps the user leaves stay. Components never float up on their own (that would move things the user did not touch).
- **Add:** a new component gets its type's default size and the first free place in reading order where it fits; if there is none, the row below everything.
- **Duplicate:** a copy with a new ID, right of the original if it fits in the same rows, else directly below it (pushing down as above).
- **Delete:** the component goes; the others do not move.
- **Narrow window:** when a column would be narrower than `Kirigami.Units.gridUnit * 3`, view mode shows the components in one column in reading order, each as high as its frame. Edit mode in a narrow window shows the same one-column list and offers only "Move earlier", "Move later", Duplicate, Delete and the properties; it says in one line that the window must be wider to arrange columns. The stored frames do not change by looking at them in a narrow window.

**Content that does not fit** in its frame scrolls inside the component, and the component says so to a screen reader; it is never cut off silently.

**Built in M6b.1** as `kassensturz-report`'s `grid` module and `Editor` (the command stack of section 2). Its own choices are D-296: default sizes (chart and table 6 × 4, key figure card 3 × 2, progress bar 6 × 1), rows ending at 1,000 with a refused edit (`report.grid-edit-refused`) changing nothing, and how Move earlier and Move later pick a place.

## 2. The editor and its undo (D-288)

- **View mode** is the normal page. **Edit mode** starts from the page's "Edit layout" action (one deliberate step, AGENTS.md §6) and shows the grid lines, a handle on each component, a resize corner, a component library to add from, and the properties panel of the selected component (the panel of M6a.6, unchanged in its rules).
- Edit mode draws the real components with the real figures (from `BookCharts` and `BookCards`), so there is no separate preview of the content. **Preview** hides the grid lines and handles without leaving edit mode.
- **Command stack:** the editor keeps its own undo and redo in memory (`kassensturz-report`, no Qt): each add, move, resize, duplicate, delete, property change and reset is one command, at most 200 of them (the oldest goes first). It never writes to the book while editing and never touches the book's history.
- **Done** writes **one** history entry for the whole session: `add_view` when the dashboard is stored for the first time (the built-in default is not stored, D-278), else `edit_view`. The book's Undo then takes back the whole session in one step. **Cancel** throws the session away; when there are changes it asks first ("Throw away your changes to the layout?").
- **Reset to the built-in layout** (in edit mode) is a command like the others: it replaces the components with the built-in default, so the editor's Undo takes it back; after Done, the stored copy is removed (`remove_view`), so the book's Undo brings it back too. (This moves the reset of M6d.5 forward for the one dashboard M6b has.)
- **Undo keys while editing** go to the editor's stack, not to the book: Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y, and the menu's Undo and Redo in edit mode.

**Keyboard and screen reader** (edit mode; every mouse action has a key):

| Key | Does |
|---|---|
| Tab, Shift+Tab | The next or previous component in reading order |
| Arrow keys | Move the focused component by one cell (pushing down as above) |
| Shift+Arrow keys | Make it one cell wider, narrower, taller or lower |
| Enter | Open its properties |
| Delete | Remove it (undoable, so no question) |
| Ctrl+D | Duplicate it |
| Escape | Stop a drag that is under way; otherwise nothing (Done and Cancel are buttons) |

A component's accessible name says what it is and where: "Chart: Income and expenses, column 1, row 1, 6 columns wide, 4 rows high". After a move or resize the new place is announced. Every handle has a text label; nothing is only in colour or only on hover (D-283).

## 3. The dashboard document v1 (D-289)

The document of D-278 stays `"version": 1`. M6b adds only keys that an older version-1 reader may ignore and a newer one fills with defaults, so no version change is needed. A change that is not additive would raise the version to 2; in Alpha an older document would then simply be refused (no migration).

- **Frames are checked** by `Document::read` for a dashboard: `col + cols ≤ 12`, `cols` 1 to 12, `rows` 1 to 24, the smallest size of the type, and no two frames overlapping. New `report.document-invalid` reasons: `frame-outside-grid`, `frame-too-small`, `frames-overlap` (with the path of the second component). A document that fails is refused whole, never repaired silently.
- **`props.period`** on every component that shows data (not the heading): `page` (the default: the page's period, e.g. the Overview's month) or one of the five fixed periods (`this-month`, `last-month`, `last-12-months`, `this-year`, `last-year`). The Overview's 12-month line uses `last-12-months` this way.
- **Formulas** (M6b.7): `props.formula` on `kpi.card` (with `measure` `formula`) and `props.formula` and `props.target` on `progress.bar`; each is text of at most 1,000 characters on one line. `Document::read` checks only the length and that it is plain text; the formula is parsed when the component is shown, and a formula with a mistake shows the mistake in words **on that component** while the rest of the dashboard opens (section 5).
- **Groups in charts** (M6b.5): `params.grouping` gains `groups` (section 4); `params.groups` lists the chosen group IDs and `params.series` the chosen category or group IDs of a line, area or stacked-bar chart over time. IDs are text in the document; an ID the book does not know is skipped and the component says "A group in this chart no longer exists". At most 50 IDs in each list (the series limit of D-281).
- **`props.show_hidden`** (flag, default off) on charts and tables: show the hidden categories in this one component anyway (section 4).
- **`text.heading`** (D-312, D-313): `props.text` only, no dataset, no params, no period; in the library, not among the components the AI may pick.
- **The built-in default document** is a constant in `kassensturz-report` that rebuilds today's Overview (Q-215 = A: the key figure cards, the net worth card, the spending donut with its five largest categories, the 12-month line) on the grid. A test reads it with `Document::read` and compares what the Overview shows with and without it.

All the limits of D-281 stay. Every new text (a formula, an ID) is data, shown as plain text, never markup or an instruction (T-37, T-41).

## 4. Category groups and the hide list (D-290, D-291)

**What the user gets.** *My groups*: the user puts categories together under a name of their own, e.g. "Fixed costs" = Rent, Insurance, Phone, and every chart type and the table can show the group as one slice, bar or line. *Hidden categories*: categories the user does not want to see in charts, e.g. Rent in a spending pie where it would crowd out everything else. Both are made once and shared by the Overview, the Statistics page and every dashboard.

**What it never does.** A group or the hide list never changes a category, a transaction, a total or a booking. It is a way of looking at the figures, nothing else.

**Storage: layout 21** (its own small step, M6b.L, like M6a.L; Alpha: a new book is built by all migrations and an older Alpha book is refused as before). Three tables, STRICT, apart from the transactions and the category tree:

`report_groups`

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `name` | TEXT | 1 to 200 characters, no spaces around it; unique among groups ignoring capitals (index folds ASCII; KassenSturz checks all letters when it writes); never the same as a category's name when it is made (so a formula name is clear, section 5) |
| `kind` | TEXT | `expense` or `income`: every category in it is of this kind, so the group's figure means one thing (money spent, or money received) |
| `position` | INTEGER, from 0 | The order in the lists, numbered 0, 1, 2, … without gaps |

`report_group_members`

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7, so the history can name the row |
| `group_id` | BLOB → `report_groups.id` | The group; removing it removes this row |
| `category_id` | BLOB → `accounts.id`, may be NULL | A category in the group; removing the category removes this row |
| `member_group_id` | BLOB → `report_groups.id`, may be NULL | A group inside the group; removing that group removes this row |
| `position` | INTEGER, from 0 | The order in the group editor |

Exactly one of `category_id` and `member_group_id` is set (checked by the database). Unique indexes on (`group_id`, `category_id`) and (`group_id`, `member_group_id`).

`report_hidden`

| Column | Type | Meaning |
|---|---|---|
| `category_id` | BLOB → `accounts.id`, primary key | A hidden category; removing the category removes this row |

Plus `changed_report_groups`, `changed_report_group_members` and `changed_report_hidden` (D-032, D-066), and the history actions `add_group`, `edit_group` (name and members), `move_group`, `remove_group` and `set_hidden_categories`; `changes` is rebuilt for them as in layouts 17 to 20. A category removed from the book takes its membership and hide rows with it in the same change, so its Undo brings them back (a test proves it). The complete export (`docs/EXPORT_FORMAT.md`, version 7) carries the groups, their members and the hidden categories by ID; a round trip changes nothing. The budgets of M6c become layout 22.

**Rules the book checks when it writes** (`store.` codes, section 7): a member category exists and is of the group's kind; a group inside a group is of the same kind; **no loops** (a group never contains itself, directly or through others); **at most 4 levels** of groups inside groups; at most 200 groups in a book and 500 members in a group; a hidden category exists and is a category (not an account).

**How they count** (in `kassensturz-report`; aggregation stays in Rust and exact):

- A category in a group counts **with everything inside it** (as the `categories` filter does, D-277): Food in a group brings Groceries and Restaurants.
- Every category counts **once** in a group, however many ways it is reached (Food and Food › Groceries both in "Household" count Groceries once). A group's figure is the exact sum of those categories' figures in each currency; groups are per currency like every dataset.
- A category may be in several groups. **Charts that show parts of a whole** (pie, donut, stacked bars, the table's shares) **refuse two chosen groups that share a category** (`report.groups-overlap`, naming the category and both groups: "Phone is in Fixed costs and in Household; a pie can't count it twice. Take it out of one group, or show the groups as bars or lines."). Charts of separate series (line, area, bars) allow it.
- With grouping `groups`, each chosen group is one slice, bar or line, and every category in none of the chosen groups shows on its own, as before.
- **The hide list removes categories only where they would be shown on their own.** A group always counts all its members, and a formula always counts what it names (section 5), so a group's figure is the same in every chart, card and formula. (The other reading, hidden categories leaving groups too, is Q-225.)
- A hidden category takes everything inside it out of view as well.
- **Hidden money is never silently missing:** a chart or table that leaves hidden categories out says so in a line under it, with the amount: "Hidden: Rent and 1 more (1,250.00 € not shown)". The text table (the screen-reader path, D-283) has the same line. The shown total is the total of what is shown and is labelled so.
- **The key figures** (money in, money out, what is left, share kept, net worth) are never affected by groups or the hide list.

**Pie, donut and ring stay readable** (M6-B1, M6b.5):

- At most **8 slices**: when there would be more, the 7 largest stay and the rest are one slice "Other" (grey), so every slice has its own colour of the 8-colour palette (D-283).
- A slice of at least 5 % of the shown total has its name on the chart; the legend lists every slice with its name, amount and share; pointing at a slice or focusing it with the keyboard shows the same text. "Other" lists what it holds in its tooltip and in the table.
- A negative row (a refund larger than the spending) can't be a slice: it is left out of the drawing and listed in the legend and the table with a note.

**Any category or group as a line** (M6-B6): line, area and stacked bars over time take `params.series`, a list of categories and groups (dataset `category_over_time`, M6a's catalog; M6b.4 builds it if it is not built yet). Money in and money out stay available as before.

**The AI sees no groups and no hide list in M6b**: no read function and no change set includes them. Whether the M6c chart builder may read them is decided there, under the grid's category-read cell (D-252).

**Built in M6b.4** (D-304): `Source::read` reads the groups and the hide list; `spending_by_category_shown` with `Shown` (groups, `show_hidden`, `Overlap`) gives the group rows, the hidden categories and their total for the note; `category_over_time` gives any categories and groups per month and for the period; `Source::counted` and `Source::unknown` say what a name counts and which chosen IDs the book no longer knows. The catalog, the document keys and the charts moved over in M6b.5 part 1 (D-305, D-306): the app's charts call `spending_by_category_shown` and `ChartModel::hidden` carries the note (`HiddenNote`); `params.groups`, `params.series` and `props.show_hidden` are in the schema (`Kind::Ids` for the lists); `category_over_time` is in the catalog and drawn as lines, areas or stacked bars (`Series::name` holds the book's name); the AI sees none of it (`DatasetInfo::for_ai`). Done in M6b.5b (D-307, D-308): the 8-slice pie, donut and ring with "Other", shares, names on slices of 5 % or more, the legend with amount and share, the pointer and keyboard sentence. Done in M6b.5c (D-309, D-310): the My groups and Hidden categories page. The panel's grouping `groups` and the group pickers wait for the grid editor's panel.

## 5. The formula language (D-292)

**Purpose.** A key figure card or a progress bar shows a figure the user writes, spreadsheet style: `=SUM(Pets; Travel)`, `=Pets - Travel`, `=PERCENT_OF(Groceries; 'Money out')`. Clean-room: the syntax follows the public descriptions of spreadsheet formulas (the OpenDocument formula standard and LibreOffice's help pages), names and argument order only; no code of any spreadsheet is read or used (AGENTS.md §3.1 rule 5). Our own small parser in `kassensturz-report`; no new dependency.

**Syntax** (case does not matter for function names, names or period words):

```
formula    = [ "=" ] test
test       = sum [ ( "=" | "<>" | "<" | ">" | "<=" | ">=" ) sum ]   one comparison at most
sum        = product { ( "+" | "-" ) product }
product    = unary { ( "*" | "/" ) unary }
unary      = { "-" | "+" } postfix
postfix    = primary [ "%" ]                      20% is 0.2
primary    = number | reference | call | "(" test ")"
call       = FUNCTION "(" [ test { SEP test } ] ")"   SEP: see below
reference  = name [ "@" period ]
name       = bare name | "'" any text, '' for a quote "'"
period     = '"' period words '"' | dates
number     = digits [ DECIMAL digits ]              no grouping, no exponent
```

- **Names.** A bare name starts with a letter and has letters, digits and `_` (any language's letters: `Lebensmittel`, `Café`). Any other name is in single quotes, as spreadsheets write sheet names: `'Eating out'`. A path names a category inside another: `'Everyday > Pets'`.
- **Separators the way LibreOffice does it** (Q-227, D-295). The book **stores** every formula in one canonical form, as OpenDocument files store theirs: `;` between arguments, `.` as the decimal mark, English function names. The formula field **shows and takes** the form of the user's number format (D-002, Settings → How amounts look): with a decimal comma (e.g. German) the separator is `;` and the decimal mark `,` (`=ROUND(Pets * 1,5; 2)`); with a decimal point (e.g. English) the separator is `,` and the decimal mark `.` (`=ROUND(Pets * 1.5, 2)`). As in LibreOffice, `;` is accepted as a separator in every number format, so a formula copied from German help text still works. Rust converts between the two forms token by token (only separator and decimal-mark tokens change; names, quoted text and spacing stay as typed), so a book moved to a computer with another language shows the same formula in that computer's form and means the same. A round-trip test covers every number format. A separator in the wrong place gets a plain error with the right form ("Put ; between the parts: SUM(Pets; Travel)"). A setting of its own for the separator, as LibreOffice has, is not part of M6b (ideas backlog).
- **Function names are English and fixed** (`SUM`, `AVERAGE`, …), stored as typed, so a book works in every language. The function list in the editor explains each in the user's language (Q-226 = A: no German names such as `SUMME` in M6b).

**What a name means.** A name is a category or one of My groups. It stands for that category's or group's figure in the period: money spent for a spending category, money received for an income category, both as positive amounts (a refund makes it smaller and can make it negative), everything inside the category included. So `=Salary - Rent` is what is left of the salary after the rent. Hidden categories count when a formula names them (section 4). Accounts are not names in wave 1 (`BALANCE` is wave 2).

- **Resolution:** a group name, a category path, or a category name. A name that fits nothing is `report.formula-unknown-name` ("Travel is not a category or group"). A bare category name used by two categories (Food › Other and Car › Other) is `report.formula-ambiguous-name` and the message lists the paths to write instead. A group and a category with the same name (possible when a category is made after the group) is ambiguous the same way.
- **Currency:** a formula's result has one currency, the currency of the categories it names. Names in two currencies are `report.formula-mixed-currencies` ("Pets is in EUR and Travel in USD; a formula uses one currency"). Converted totals wait for the converted datasets (D-277).

**The time range.** The figure's period is the component's `props.period` (section 3), so `=Pets - Travel` needs no range. `name@period` gives one name its own range:

- Period words in double quotes: `"this month"`, `"last month"`, `"this quarter"`, `"last quarter"`, `"this year"`, `"last year"`, `"last 12 months"`. They count **from the figure's period**, not from today: on the Overview showing March 2026, `Pets@"last month"` is February 2026 (as the cards' comparison counts, D-285).
- Dates: `@2026` (a year), `@2026-03` (a month), `@2026-01..2026-03` (months, both included), `@2026-01-15..2026-02-14` (days, both included). An end before the start is `report.bad-period`, a range longer than 100 years `report.period-too-long` (the existing codes).

**Kinds of values and exact arithmetic.** Every value is an **amount** (with a currency), a **number**, or a **yes/no** (only from a comparison, used inside `IF`, `AND`, `OR`, `NOT`). The kind of every part is known before any figure is read, so most mistakes are found while typing.

| Operation | Result |
|---|---|
| amount ± amount (same currency) | amount |
| amount ± number | amount (the number is an amount in that currency: `Pets - 50`) |
| amount × number, amount ÷ number | amount |
| amount ÷ amount | number (a ratio) |
| amount × amount, number ÷ amount | refused: `report.formula-kinds` ("Two amounts can't be multiplied") |
| a comparison | yes/no; compares the exact values |
| a yes/no in arithmetic, or as the whole result | refused: `report.formula-kinds` |

- Values are **exact decimals** (`rust_decimal` behind our own type, D-051; no floating point anywhere, AGENTS.md §3.1 rule 1). Amounts start as exact minor units. `+`, `-` and `×` are exact and overflow-checked (`report.formula-too-large`).
- **Division is the only rounding inside a formula:** its result is rounded to 10 decimal places, half away from zero, a fixed rule written in the code. A division by zero gives **no value** (`report.formula-no-value`, "This divides by zero; use IFERROR to show something else"), never 0 or infinity.
- **The result is rounded once, at the end:** an amount to its currency's minor unit with the book's rounding rule (D-027); a number shown as a percent to one decimal place of a percent, half away from zero (D-285); any other number to two decimal places, half away from zero. The figure leaves Rust as text (D-065).
- A number is **shown as a percent** when it comes from `%`, `PERCENT_OF` or `CHANGE` and keeps that through unary minus, `ABS`, the rounding functions, and `IF`, `MIN`, `MAX` and `AVERAGE` when all their values are percents.

**Wave 1 functions** (M6b.6; 1 to 30 arguments where a list is allowed):

| Function | Gives |
|---|---|
| `SUM(x; …)` | The sum |
| `AVERAGE(x; …)` | The sum divided by the number of values (the division rule above) |
| `MIN(x; …)`, `MAX(x; …)` | The smallest, the largest |
| `COUNT(name; …)` | How many transactions in the period have a part in one of the named categories or groups, each transaction once (a Transfer has no category, so it never counts) |
| `ABS(x)`, `SIGN(x)` | Without its sign; −1, 0 or 1 |
| `ROUND(x; places)`, `ROUNDUP(x; places)`, `ROUNDDOWN(x; places)` | Rounded half away from zero, away from zero, toward zero; `places` a whole number from −9 to 10 (−2 rounds to hundreds) |
| `INT(x)` | Rounded down to a whole number |
| `IF(test; yes; no)` | `yes` or `no`; both must be the same kind; only the chosen one is worked out |
| `AND(test; …)`, `OR(test; …)`, `NOT(test)` | Yes/no |
| `IFERROR(x; fallback)` | `fallback` when `x` has no value or is too large; it does not hide a mistake in the formula (unknown name, wrong kind) |
| `PERCENT_OF(part; whole)` | `part ÷ whole`, shown as a percent; no value when `whole` is zero |
| `CHANGE(new; old)` | `(new − old) ÷ old`, as the cards' comparison counts it (D-285), shown as a percent; no value when `old` is zero |

Wave 2 (`MONTHLY`, `MEDIAN`, `PREVIOUS`, `YEAR_AGO`, `SUMIF`, `COUNTIF`, `TOP`, `SHARE`, `BALANCE`, `NETWORTH`, later `BUDGET`) follows in M6b if the budget allows, else in M6c. **Never:** functions with inexact results (`SQRT`, `LOG`, `POWER` with a fraction, statistics that need roots), random or changing values (`RAND`, `TODAY` as a value), anything that reaches outside the book's figures (`INDIRECT`, `WEBSERVICE`, files, databases), text and pattern functions, macros and scripts.

**Limits** (checked before anything is read; over one is `report.formula-too-big` with the limit's name): 1,000 characters, 500 tokens, 16 levels of brackets and calls, 30 arguments in a call, 64 names, 8 different periods. A formula reads only figures of the reporting layer for the names it holds (one read of the book for all its periods); it writes nothing. There are no named formulas, so a formula cannot refer to itself or to another formula, and no loop is possible.

**Mistakes in plain words.** Every mistake is an error code with its parameters (D-034), never text from Rust: where it is (the character position, from 1), and the name or function concerned as data. The card or bar shows the message instead of a figure; the formula field (M6b.7) marks the place. A syntax mistake says what was expected ("A ) is missing after Travel").

**Trust, for the everyday user** (M6b.7): the formula field lists the functions with one-line explanations, completes category and group names while typing, and shows the result next to the formula with what it counted: "= 312.40 € · counts Pets (with Vet), Travel". So nobody has to trust a formula blindly.

## 6. Editing AI-prepared changes (#191, D-293)

Today a change set (D-047, D-199) can only be confirmed whole or not at all, so one wrong detail (for example a right category under the wrong parent) means asking again. M6b.8 lets the user correct a prepared change on the review page before confirming.

- **What can be edited:** a new category's name and parent; a new Rule's name, category and conditions (with the Rule editor's own fields); the category of a recategorization, and taking single transactions out of its list; a new chart's title, chart type and period. Any change can also be left out of the set.
- **The same checks as the AI's proposal:** an edited change goes through the same validation as a proposed one (`propose`'s checks: names, parents in the book or in the same set, kinds, limits), and `check_allowed` asks the permission grid again at Confirm. An edit can only change a field of the same kind of change; it can never turn a change into another kind or add a kind the grid does not allow.
- **Still AI-made:** the change stays labelled AI-made (D-198) and goes into the book as the one `apply_ai_changes` history entry, which one Undo takes back. The review page marks an edited change "changed by you" and offers "Back to the AI's version". No layout change: the mark lives only on the review page until Confirm.
- **Nothing goes back to the model:** the edits are not sent to the AI. Numbers in the preview still come from code (§8).
- With AI off, none of this is visible or reachable (D-013).

## 7. Capability check in "Test connection" (D-294)

"Test connection" on the AI settings page today lists the provider's models (D-182). M6b.9 also says, in plain words, what the chosen model can do for KassenSturz:

- **Can it use KassenSturz's functions** (function calling)? A fixed, synthetic test request with one harmless test function and the words "Call the test function". It contains **nothing from a book**: no names, no amounts, no categories. Yes when the reply calls the function with the expected argument, no otherwise. Without functions the chat uses the small precomputed overview (D-052) and says so.
- **How much can it read at once** (the context size)? Taken from the model list when the server gives it there (some OpenAI-compatible servers add a field for it); otherwise "not known". Nothing is guessed.
- The button says before it runs what the test sends ("a short test question, nothing from your books"), the same consent rules apply (D-035), and the reply is read with the same limits as every reply (T-31). With the fake AI (`KASSENSTURZ_FAKE_AI=1`) nothing is sent and both answers come from the fake model. With AI off nothing of it runs.
- The result is kept with the provider's settings on this computer (D-168), never in the book, and shown again until the next test.

## 8. Shared QML components (the M6a retro rule)

When two side-by-side steps both add QML components, both briefs carry this list, or the owner named here builds the shared one.

- **Existing:** `ChartView`, `ChartTable` (wraps `DataTable`), `DataTable`, `ChartPalette`, `KpiCard`, `KpiGrid`, `KpiNames`, `ProgressMeter`, `ComponentPreview`, `PropertiesPanel`, `OverviewMonth`, `OverviewPage`, `StatisticsPage`, `AccountGrid`, `AiChangeReviewPage`, `AiRequestReview`, `AiPermissionGrid`, `ImportPreviewPage`.
- **Planned, with their owner:** M6b.C owns the one table component (`DataTable`, with `ChartTable` folded in or kept as its only wrapper) and the bar labels in `ChartView`; M6b.3 `DashboardGrid`, `GridItemFrame`, `ComponentLibrary`; M6b.5 `CategoryGroupsPage` (My groups and Hidden categories) and the legend and "Other" in `ChartView`; M6b.7 `FormulaField`; M6b.8 `ChangeEditor` inside `AiChangeReviewPage`; M6b.9 changes only `AiSettingsPage`.
- **Full category names in bars** (M6-B3, M6b.C): a bar's name wraps to two lines; when it still does not fit, the chart turns to horizontal bars; a name is shortened only beyond that, and then the full name is in the tooltip, the accessible name and the table.

## 9. Risks and their rows in the threat model

New rows T-41 to T-45 in `docs/THREAT_MODEL.md`: a hostile formula, a formula name that misleads, groups or the hide list that make figures wrong or hide money, an edited AI change that slips past the checks, and the capability probe. T-37 also covers the new frame checks (M6b.2).

## 10. The M6b steps

The table is in `docs/ROADMAP.md` ("M6 steps", rows M6b.\*), with needs, tier, effort and which steps run side by side (D-286).
