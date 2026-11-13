# The money-flow chart (US7, #821, D-758)

Design of User Sprint 7 item #821, written by thread C in Phase 2 before its build. It fixes the nodes, the rules that keep both sides equal, the balance node, the Builder type and the order of the build steps, so the code steps don't have to guess.

This is our own chart, built from the general idea of a flow diagram. It copies no other app's look, names or wording.

Release stage: **Alpha** (D-274). The chart lives in a dashboard document's settings, so **the book format is not touched**: no layout change, no migration, not a **[format]** item, no human gate. The document `VERSION` stays as it is: an older build shows the tile as a placeholder (D-278, D-504).

**What stays true:** the start screen is complete without it (D-342): the tile is in the library, not in the built-in Overview or a template. **Numbers come from code** (AGENTS.md §8): every amount is worked out in Rust from exact `Money`, in minor units. QML gets text and drawing coordinates only (D-065); a drawing value is never read back, added up or shown.

## 1. What it shows

One chart for one period and one currency. Money enters on the left, passes **one middle node, *Your money* (German *Ihr Geld*)** and leaves on the right. The thickness of a band is its amount; the same scale is used everywhere in the chart.

| Column | With the options off | Options |
|---|---|---|
| 1 | *(not drawn)* | **Payees** who paid (*Show payees*) |
| 2 | Income categories (top level) | **Sub-categories** split a top-level category into a column of its own, behind it |
| 3 | **Your money** | the chosen accounts instead (*Show accounts*, §6) |
| 4 | Spending categories (top level) | **Sub-categories** as column 5 |
| 5 | *(not drawn)* | **Payees** paid |

A top-level category that has bookings of its own as well as sub-categories gets a sub-node named after it (*"Food itself"*; the screen writes the wording, D-034).

## 2. What counts

Everything comes from `Source::parts` (the one place that decides what is income or spending, D-122), so the chart counts exactly what the overview counts: Transfers between the user's own accounts and starting balances are **left out** (money that moves through several accounts is counted once, not at every hop). The existing transfers note explains this under the chart. The chart's filters (accounts, categories, tags, the dashboard's filter bar) are the ones every chart has (D-350).

- **Netting.** Per category, income parts and spending parts are summed with their signs. A refund reduces its category's spending. A spending category whose net is money coming back (more refunded than spent) is drawn on the **income side**, and an income category whose net is negative on the **spending side**. A category is never drawn on both sides.
- **Not sorted yet.** Parts without a category get one node, *Not sorted yet*, on the side their sign says (as the overview counts them). It is never folded into *Other*.
- **Payees.** Per category the payees' nets are the bands between the payee column and the category. A payee whose net in a category has the *opposite* sign to the category's own is folded into that category's *Other* band, which is then netted; if that sum would not be on the category's side, the smallest payees of the side are added to it until it is. This keeps every band positive and every sum exact. A payee shared by several categories is **one node**.
- **One currency.** The chart shows the currency named in its settings (`auto`: the currency with the most parts, ties by code). Nothing is converted. What is booked in other currencies is named in a line below the chart ("also 120.00 USD, not shown"), one text per currency, written in Rust.
- **At most 8 nodes per column.** The 7 largest by amount stay, the rest become *Other (n)* with n their number. *Not sorted yet* counts as one of the 8. What *Other (n)* holds is listed in the table view, largest first.
- **No budget layer.** A category can sit in several budgets, so a layer would count money twice or pick a budget arbitrarily. It can come back if testers ask.

## 3. The balance node

Money in and money out of *Your money* must be equal, so both sides always add up to the same total *T*:

- income is larger than spending: the difference flows on to **Left over** (*Übrig*), a node at the end of the spending column;
- spending is larger: **From your reserves** (*Aus Ihren Rücklagen*), a node at the end of the income column, makes up the gap;
- equal: neither node.

Neither node continues to payees. The invariant, tested as a property: **for every node, the bands in equal the bands out** (for nodes in the first and last column, the node's own amount), and every column adds up to *T*.

## 4. Layout (Rust, in `kassensturz-charts`)

The report crate (`money_flow.rs`, dataset id `money_flow`) returns nodes and flows in exact minor units. The charts crate turns them into a `FlowModel` (`flow.rs`) whose positions are **whole millionths of the chart's height**, worked out with integers (`amount × room / total`, rounded down, so a node's bands never add up to more than it); QML divides by `WHOLE` to place marks and never reads them back:

- columns left to right as in §1; inside a column the nodes go largest first, *Other (n)* and *Not sorted yet* last, the balance node last of all;
- every column adds up to *T*, so one scale fits all: `scale = (1 − widest gaps) / T`; a column with fewer nodes is centred;
- a node's outgoing bands are ordered by the position of their target, its incoming bands by the position of their source, so bands cross as little as simple ordering allows;
- a band is `{ from node, to node, y at start, y at end, thickness }`. A band thinner than a hair is drawn at a minimum pixel width by QML (drawing only, the layout and the amounts don't change);
- no floating-point arithmetic is used in Rust at all.

## 5. Where it lives

- **A Statistic Builder chart type** (D-741 requires one): component type `chart.flow`, kind word `flow`, in `ComponentType::BUILDER`, the Builder's three QML lists and `LIBRARY`. Settings: accounts, period, `sub_categories`, `payees`, `show_accounts`, `currency`. Default size 6 × 4, smallest 4 × 3.
- **A ready-made tile** *Where my money went* (*Wohin mein Geld ging*) under *Add › Charts*: this month, all accounts, sub-categories off, payees off. **Not** on the built-in Overview or a template (D-342).
- **AI:** in `ComponentType::ALL`; the chat proposes it through `propose_view` like any other chart, behind the existing permission cells (the Builder test "the Builder makes every component the AI may pick" covers it). It does not fit the custom chart description (D-504), so it is its own type. It gets no figures from the model: the chart is computed in Rust from the book.
- **Drawing:** QtQuick Shapes, as `ChartOverlays.qml` already uses: rectangles for nodes, cubic-curve bands for flows (`FlowChart.qml`, shown by `ChartView` when `chart.kind` is `flow`). KQuickCharts and Qt's chart modules have no such chart.
- **Click** a band: the transactions behind it open, as for the other charts (the category, the payee and the period as filters). A click on a node opens everything that flows through it. **Hover** shows the amount as text from Rust.
- **Accessibility:** a table of every flow (*From*, *To*, *Amount*) behind "Show as table", the other charts' control; every band and node has a screen-reader label with the same text. Colour is never the only difference: income bands and spending bands also differ by side and by the table's column, and the first four colours carry a second mark as in the other charts.

## 6. *Show accounts* (the last step)

Off by default. With it on, the chosen accounts replace *Your money*:

- income flows into the account it arrived in; spending flows out of the account it was paid from;
- a Transfer between two chosen accounts is a band between the two account nodes; a Transfer to or from an account that is not chosen is a band to or from *To / From other accounts*;
- each account node has its own *Left over* or *From your reserves* band, so per account the bands in equal the bands out exactly;
- account columns may form chains (A → B → C). The layout orders accounts so most Transfer bands go left to right; a band going back (a card paid from the current account) is drawn as a curved return band under the nodes.

Here the Transfers are *not* left out, which is why this layer needs the own-account side of each transaction and is built after the basic chart works.

## 7. Build steps

1. **Rust data and layout:** `money_flow` dataset in `kassensturz-report` (columns 1 to 5, netting, payees, *Other (n)*, balance node, other currencies) and `FlowLayout` in `kassensturz-charts`; unit and property tests. Component type `chart.flow` with its settings, in `BUILDER`.
2. **QML drawing:** `FlowChart.qml`, hover, click-through, the table view and screen-reader labels; the bridge object read back the way QML reads it.
3. **Builder entry, tile, AI, help:** the Builder's three lists, the library tile, `propose_view`, the help text, glossary, English and German strings.
4. **Show accounts**, with return bands.

Each is a pull request that merges on green. Tests use synthetic books and invented names only.
