# Design delta: the grid editor and System Monitor's page editor (M6b)

Written in Sprint M6b, Phase 1, as a follow-up to `docs/REPORT_DESIGNER_M6B.md` (D-286 to D-296). The maintainer asked that the dashboard grid be modelled on the customizable pages of KDE's System Monitor (`plasma-systemmonitor`), but for charts and financial reports. This note says what we study, what we adopt, what we leave, and what changes in the steps. Decisions D-311 and D-312 and questions Q-239 and Q-240 are **Proposed / open** until the maintainer answers; the safe defaults below hold meanwhile.

## 1. What was studied, and how

Read from the KDE mirror `github.com/KDE/plasma-systemmonitor` (master, 2026-10-29; `invent.kde.org` is not reachable from the agent environment): `src/page/` `PageEditor`, `RowControl`, `ColumnControl`, `SectionControl`, `FaceControl`, `Container`, `EditorToolBar`, `MoveButton`, `EditablePage`, `FaceConfigurationPage`, `PageDataObject.h`, `PageManager.cpp`. The app was **not run**, so how drag feels is not judged here. This is behaviour study for a clean-room design (AGENTS.md §3.1 rule 5); no code is copied.

**How System Monitor builds a page**

- **Structure is a tree, not frames.** A page holds *rows*; a row is either a *title* (a heading) or a row of *columns*; a column holds *sections* (stacked, optional separators); a section holds one *face* (a plugin that shows sensors, with its own configuration). Columns of a row are always equal in width; there is no free size or position.
- **Heights are computed.** Each row has a `heightMode` (`minimum`, `balanced`, `maximum`, `half`); a `relayout()` function divides the window height between rows, so a page fills the window without scrolling. That suits a monitor.
- **Edit mode** (`Edit Page`, `Save Changes`, `Discard Changes`, `Add Row`, `Add Title`, `Configure Page…`): clicking an item selects it and a toolbar appears *on that item only* (add column, height, move handle, remove). Moving is a drag on the toolbar's handle; items animate to their new place (`Behavior`/`Transition` with `Kirigami.Units.shortDuration`).
- **Face configuration** opens as a further page in the Kirigami page stack, with *Load preset*, *Save settings as preset* and *Get new presets* (KNewStuff, a download store).
- **Problems are shown once, at the top:** a `Kirigami.InlineMessage` "This page is missing some sensors…" with a "Fix…" action.
- **Saving:** the page is a tree of `PageDataObject` (a `QQmlPropertyMap` with dirty tracking), written to a `.page` file (KConfig) on Save.
- **Not found in the files read:** any `Accessible` name, `Keys` handler or `Shortcut` for the page editor. Editing is mouse only. Our D-288 (every mouse action has a key; place announced) is already ahead of it.
- **Licence:** the QML and C++ files are `LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL`, the README `BSD-3-Clause`; compatible with `GPL-3.0-or-later` through the LGPL-3.0 option. But the page editor is **app-internal** (`org.kde.ksysguard.page` is built into the app, not a library), tied to `PageDataObject`, KConfig and faces, so it cannot be imported; only copied, which AGENTS.md §3.1 rule 5 and §3.2 gate (maintainer approval, header and `docs/THIRD_PARTY.md`). Copying would save little (`EditorToolBar` and `MoveButton` are about 240 lines and move in one dimension; ours moves in two with snapping).

## 2. Where the idea meets D-287 to D-296

| Question | System Monitor | KassenSturz (D-287, D-289, D-296) | Verdict |
|---|---|---|---|
| Structure | Rows, then equal columns, then sections | A flat list of components with `col`, `row`, `cols`, `rows` on 12 columns | **Keep ours.** Any rows-and-columns layout is a set of frames, so ours can express theirs; theirs cannot express a chart spanning 8 columns beside a 4-column card over two rows. |
| Size | Equal share of the row; heights computed from the window | Whole cells, 1 to 24 rows, a row is `gridUnit * 4` | **Keep ours.** Reports are read, scrolled and printed; filling the window suits a live monitor, not a month's figures. Their `heightMode` is left out. |
| Overlap | Impossible by structure | Impossible by push-down, checked in `grid::check` | Same result, ours already built and tested (#246). |
| Narrow window | Not a design topic (columns shrink) | One column in reading order (D-287) | Keep ours. |
| Selection | Toolbar only on the selected item | Handles on every component in edit mode | **Adopt theirs** (section 3). |
| Titles | `Add Title` row | None | **Adopt** as a Heading component (D-312, Q-240). |
| Config | Side page with presets and a download store | Properties panel (M6a.6) | Keep ours. Presets: a later idea. The store conflicts with "no plugin store" (D-007) and with AGENTS.md §3.1 rule 4. |
| Saving | Whole page file on Save | One history entry on Done (D-288) | Keep ours (the book's Undo takes back a session). |
| Many pages | Page list, sort dialog, `Configure Page…` (title, icon, margins) | One dashboard in M6b; several in M6d | Look at their page list again when M6d.1 is designed. |
| Keyboard, screen reader | None found | Full (D-288) | Keep ours. |

**Where the idea conflicts or costs a lot.** Switching to rows and columns would throw away M6b.1 (`grid`, `Editor`, their tests, D-287, D-296, the frame checks planned for M6b.2) and the free sizing the grid gives. That is about two strong-tier steps again (the model and the document) plus rewriting D-287/D-288/D-296 and the M6b.3 brief, for a layout that is *less* able; its only gains are simpler reading order and a simpler narrow window, which D-287 already solved. It is not recommended (Q-239 offers it anyway, because the layout is the maintainer's to choose).

## 3. What to adopt (D-311)

1. **A toolbar on the selected component only** (as `Container`/`EditorToolBar` do), instead of a handle and a resize corner on every component: Move (drag handle), Duplicate, Remove, Properties, and the move-earlier/move-later pair in a narrow window. The resize corner and keyboard map of D-288 stay. Fewer visible controls is the progressive-disclosure rule (AGENTS.md §6); the toolbar is a real `Kirigami.ActionToolBar`, so it is keyboard- and screen-reader-operable by construction. Edit mode still shows the grid lines.
2. **Animated moves.** Components glide to their new place when pushed down or moved, with `Kirigami.Units.shortDuration` (which is 0 when the user turned animations off). This also shows *what* a push-down moved.
3. **One problem message at the top** (`Kirigami.InlineMessage` with a "Show" action that selects the first affected component) when components have a problem: a formula with a mistake (M6b.7), a group that no longer exists (D-289). The per-component message stays; the top message only counts and leads there.
4. **Study, not copy:** the toolbar and drag handle are re-written for two dimensions and snapping.

**Left out:** rows/columns/sections as structure, `heightMode` and fill-the-window layout, face plugins, presets and "Get new presets", KConfig page files, `QQmlPropertyMap` data objects, copying any of their code.

## 4. What changes in the steps

| Step | Change | Effort |
|---|---|---|
| M6b.1 (merged, #246) | None. | 0 |
| M6b.2 (document v1) | If D-312 is accepted: the component type `text.heading` (below) in `grid::ComponentType` (smallest 3 × 1, default 12 × 1), `Document::read` checks `props.text` (plain text, at most 200 characters), one hostile-document case in the T-37 tests. | about +10 % |
| M6b.3 (QML grid editor) | Selected-item toolbar instead of handles everywhere; animated moves; top problem message; the Heading in the library, edited in place or in the properties panel, heading level for screen readers. `GridItemFrame` gets the toolbar. | about +15 % |
| M6b.5b/5c, M6b.7 | They only need to set a per-component problem the page can count (a `problem` text role); no other change. | small |
| M6d.1 | Reads System Monitor's page list again (section 2). | none now |

No new step, no new dependency, no new network access. These are my estimates in the roadmap's step-size terms; no tokens were measured.

**The Heading component (D-312).** `text.heading`: one line of text the user types (a section title such as "Spending" or "Net worth"), shown as a heading, plain text only, no data and no period. It lets a report be split into named sections, as System Monitor's title rows do. It is additive to dashboard v1 (D-289; a reader that does not know the type keeps it as an unknown 1 × 1-minimum component, D-296), cannot be made by the AI (`propose_view` makes charts only, D-282), and its text is data, never markup or an instruction (T-37). The one-row height is `gridUnit * 4`, taller than a title needs; accepted for now, so titles need no special case in the grid.

## 5. Questions for the maintainer (through the coordinator)

- **Q-239. Keep the 12-column frame grid, or switch to System Monitor's rows and columns?** A (R): keep the grid and adopt the three UI ideas of section 3. B: rows and columns, equal widths, heights from the window; costs about two strong steps and drops free sizing.
- **Q-240. Add the Heading component in M6b?** A (R): yes, in M6b.2 and M6b.3. B: later (M6d), the M6b library has charts, tables and cards only.

Safe default until answered: A in both.
