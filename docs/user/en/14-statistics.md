# 14. Statistics and charts

*With the Statistic Builder you build charts, tables and figures about your money, save them under a name and put them on a page. All figures come from your transactions and are exact to the cent.*

## Building a new statistic

Open the **Statistic Builder** (**Guide me step by step**). A **New statistic** takes three steps; the list of steps is on the left, and **Back**, **Next**, **Done** and **Cancel** are at the bottom. The **Preview** is under the questions in every step, so you see straight away what your choice does.

1. **Period:** you can choose **This month**, **Last month**, **The last 12 months**, **This year**, **Last year** and a period of your own, from and to (see below).
2. **Choose categories:** you say which categories count, one by one or as a group (page 6).
3. **Look and preview:** the kind of chart, the title, the legend and the name.

**Advanced: formulas and fine tuning** and **Advanced: pick from lists** hold what you usually don't need. Empty fields show pale examples and never show errors straight away. **Show all settings at once** opens everything on one page.

If you leave the builder before **Done**, nothing is lost while KassenSturz is running: the statistic is back at the same step the next time you open it. **Cancel** starts afresh.

## The kinds of chart

| Kind | Shows |
|---|---|
| **Bars**, **Stacked bars** | amounts per category or per month; stacked, you see the parts of a whole |
| **Pie**, **Ring** | shares of a total |
| **Line**, **Filled line** | how an amount changes over time |
| **Table** | the figures themselves, with a total |
| **One figure** | a single figure, also from a formula (page 15), compared with the period before |
| **Progress bar** | how far you are towards a goal, from a formula |

When you switch the kind, only the picture changes; the figures and totals stay the same.

**Custom charts:** a dashboard can also hold a **Custom chart**, drawn from a short description in the Vega-Lite language. In its **Properties**, **Insert an example** puts a ready description into the field, for example the balance as bars with money in and out as lines; you can change it there, and it is kept only when you choose **Use this description**. Which data names and fields there are is shown on the help page **Help: patterns, formulas and charts** under **Settings**.

**Direction of the bars:** in a bar chart's **Properties**, **Bars** is set to **Automatic** (rows only when the names are too long for the bars), **Across (one row per name)** or **Upright (names below)**.

## Periods that move with today's date

Wherever a period is asked for, after the quick choices there are **The last months…** and **A salary month…**:

- **The last months:** you choose how many. **Only complete months (leave out this month)** is ticked at first, because a month that isn't over yet makes the figures look small.
- **A salary month:** runs from a day of your choice, for example payday, to the day before it in the next month. If the day doesn't exist in a short month (the 31st in April), the last day counts. You choose the current one or the one before.
- **One period for the whole page:** if you choose it in the filter bar, every chart, table, number card and formula that follows the page counts the same days.

## Spending as an average

In the **Properties** of a pie, ring, bar chart or table of spending there is **Show as**: **Total** or **Average per month**, **Average per quarter** or **Average per year**. It divides by the months of the period up to today: "The last 3 months" with complete months is divided by 3; this year, in November, by 10, because December hasn't happened yet. Each amount and the total are divided once from the exact amounts.

## Money in and out with lists of your own

A chart of money in and out can be drawn as **Bars** side by side. In its **Properties**, **Money in: only these categories**, **Money in: only these accounts**, **Money out: only these categories** and **Money out: only these accounts** count only what you tick, for that side alone. Nothing ticked means: that side counts like the rest of the chart. An account's money out also includes what isn't sorted yet, and leaves out Transfers between your own accounts.

## Saving, finding again, changing

- Your statistics are under **Saved views**. **Change in the wizard:** (period…, categories…, look and title…) opens one again in the steps; **Done** keeps the change.
- **Rename**, **Delete** and **Save as a new view** are there too. Everything can be taken back with **Undo**.
- **Done** without a name keeps a new statistic as "Statistic 1", "Statistic 2" and so on.
- **Put it on the Overview** (under an open view) puts it on the overview as a chart of its own. A later change to the saved view doesn't reach this chart.
- You change a chart on a page with **Edit layout**, **Properties**, **Change step by step in the Statistic Builder**.

## Colours

All charts share one palette: **Settings › Chart colors**. **Edit palette…** (in the builder's last step and in a chart's Properties) takes you there. KassenSturz warns you when a colour is hard to see against the background.

**If something looks different:** "There is nothing to show for this period yet" means there are no matching transactions on the chosen days. Choose another period or enter transactions.
