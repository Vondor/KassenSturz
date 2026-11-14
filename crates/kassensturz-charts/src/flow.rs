// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The layout of the money-flow chart (US7, #821, D-758,
//! `docs/MONEY_FLOW.md`): the nodes and bands of `kassensturz-report`'s
//! [`MoneyFlow`] placed in the unit square. QML only scales the coordinates
//! and draws; every amount reaches it as text (D-065). The coordinates are
//! whole millionths of the chart's height, worked out with integers from the
//! exact amounts, so no floating-point arithmetic is needed here; QML divides
//! them by [`WHOLE`] to place marks and never reads them back as money.

use kassensturz_report::{FlowNodeKind, MoneyFlow, TextFormat};

/// The height of the chart in the units of the coordinates.
pub const WHOLE: i64 = 1_000_000;

/// The space between two nodes of a column, as a share of the chart's height.
const GAP: i64 = WHOLE / 50;

/// A node, placed.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidNode {
    /// What it stands for; the screen names the kinds that have no name of
    /// their own (D-034).
    pub kind: FlowNodeKind,
    /// The name from the book, or empty.
    pub name: String,
    /// The column from the left, 0 up, with no empty column between.
    pub column: usize,
    /// Where it starts and how tall it is, in millionths of the chart's
    /// height.
    pub y: i64,
    /// See `y`.
    pub height: i64,
    /// Its amount written for the user's locale, with the currency.
    pub label: String,
    /// What *Other (n)* holds, written for the user's locale: a name (empty
    /// for no payee or money not sorted yet) and an amount.
    pub holds: Vec<(String, String)>,
    /// For a category, its ID as text for a click to open its transactions.
    pub category: Option<String>,
    /// For an account node, its ID as text for a click to open its
    /// transactions.
    pub account: Option<String>,
}

/// A band, placed.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidBand {
    /// The index in [`FlowModel::nodes`] it leaves.
    pub from: usize,
    /// The index it enters.
    pub to: usize,
    /// Where it leaves its node and where it enters the other, in millionths
    /// of the chart's height (the top edge of the band).
    pub y_from: i64,
    /// See `y_from`.
    pub y_to: i64,
    /// How thick it is, in millionths of the chart's height.
    pub thickness: i64,
    /// Its amount written for the user's locale, with the currency.
    pub label: String,
}

/// The money-flow chart: what to draw and the text to say about it.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowModel {
    /// The currency of every amount.
    pub currency: String,
    /// How many columns there are.
    pub columns: usize,
    /// The nodes, the middle one first (with accounts: no middle node).
    pub nodes: Vec<LaidNode>,
    /// The bands, largest first.
    pub bands: Vec<LaidBand>,
    /// What passes through the middle node, for the chart's description.
    pub total: String,
    /// What was booked in other currencies and is not shown, one text per
    /// currency, written for the user's locale.
    pub elsewhere: Vec<String>,
}

fn rank(kind: FlowNodeKind) -> u8 {
    match kind {
        FlowNodeKind::Other => 1,
        FlowNodeKind::Unsorted => 2,
        FlowNodeKind::LeftOver | FlowNodeKind::Reserves => 3,
        _ => 0,
    }
}

/// Places `flow` in the unit square. `elsewhere` names what other currencies
/// hold (see [`FlowModel::elsewhere`]).
pub fn flow_model(flow: &MoneyFlow, elsewhere: &[MoneyFlow], text: &impl TextFormat) -> FlowModel {
    let mut columns: Vec<i8> = flow.nodes.iter().map(|node| node.column).collect();
    columns.sort_unstable();
    columns.dedup();
    let column_of = |column: i8| columns.iter().position(|&c| c == column).unwrap_or(0);

    // The order inside each column.
    let mut order: Vec<Vec<usize>> = vec![Vec::new(); columns.len()];
    for (index, node) in flow.nodes.iter().enumerate() {
        order[column_of(node.column)].push(index);
    }
    for members in &mut order {
        members.sort_by(|&a, &b| {
            let (a, b) = (&flow.nodes[a], &flow.nodes[b]);
            rank(a.kind)
                .cmp(&rank(b.kind))
                .then_with(|| b.amount.minor_units().cmp(&a.amount.minor_units()))
                .then_with(|| a.name.cmp(&b.name))
        });
    }
    // One scale for the chart: the column with the most gaps decides. An
    // amount becomes millionths of the height as `amount * room / total`,
    // rounded down, so the bands of a node never add up to more than it.
    let most_nodes = i64::try_from(order.iter().map(Vec::len).max().unwrap_or(1)).unwrap_or(1);
    let room = i128::from(WHOLE - GAP * (most_nodes - 1).max(0));
    let total = i128::from(flow.total.minor_units()).max(1);
    let size = |minor_units: i64| -> i64 {
        i64::try_from(i128::from(minor_units) * room / total).unwrap_or(0)
    };

    let mut place = vec![(0_i64, 0_i64); flow.nodes.len()];
    for members in &order {
        let heights: Vec<i64> = members
            .iter()
            .map(|&index| size(flow.nodes[index].amount.minor_units()))
            .collect();
        let gaps = GAP * i64::try_from(members.len().saturating_sub(1)).unwrap_or(0);
        let used = heights.iter().sum::<i64>() + gaps;
        let mut y = (WHOLE - used) / 2;
        for (&index, height) in members.iter().zip(heights) {
            place[index] = (y, height);
            y += height + GAP;
        }
    }
    let mut nodes: Vec<LaidNode> = Vec::with_capacity(flow.nodes.len());
    for (index, node) in flow.nodes.iter().enumerate() {
        nodes.push(LaidNode {
            kind: node.kind,
            name: node.name.clone(),
            column: column_of(node.column),
            y: place[index].0,
            height: place[index].1,
            label: text.amount(node.amount),
            holds: node
                .holds
                .iter()
                .map(|(name, amount)| (name.clone(), text.amount(*amount)))
                .collect(),
            category: node.category.map(|id| id.to_string()),
            account: node.account.map(|id| id.to_string()),
        });
    }

    // Bands: a node's outgoing bands are ordered by where their targets are,
    // its incoming bands by where their sources are, so they cross as little
    // as simple ordering allows.
    let mut out_cursor: Vec<i64> = place.iter().map(|&(y, _)| y).collect();
    let mut in_cursor = out_cursor.clone();
    let mut by_target: Vec<usize> = (0..flow.flows.len()).collect();
    by_target.sort_by(|&a, &b| {
        place[flow.flows[a].to]
            .0
            .cmp(&place[flow.flows[b].to].0)
            .then_with(|| a.cmp(&b))
    });
    let mut y_from = vec![0_i64; flow.flows.len()];
    for &band in &by_target {
        let f = &flow.flows[band];
        y_from[band] = out_cursor[f.from];
        out_cursor[f.from] += size(f.amount.minor_units());
    }
    let mut by_source: Vec<usize> = (0..flow.flows.len()).collect();
    by_source.sort_by(|&a, &b| y_from[a].cmp(&y_from[b]).then_with(|| a.cmp(&b)));
    let mut y_to = vec![0_i64; flow.flows.len()];
    for &band in &by_source {
        let f = &flow.flows[band];
        y_to[band] = in_cursor[f.to];
        in_cursor[f.to] += size(f.amount.minor_units());
    }
    let mut bands: Vec<LaidBand> = flow
        .flows
        .iter()
        .enumerate()
        .map(|(band, f)| LaidBand {
            from: f.from,
            to: f.to,
            y_from: y_from[band],
            y_to: y_to[band],
            thickness: size(f.amount.minor_units()),
            label: text.amount(f.amount),
        })
        .collect();
    // Largest first, so the screen reader and the table start with the
    // biggest flows; stable for equal bands.
    let sizes: Vec<i64> = flow.flows.iter().map(|f| f.amount.minor_units()).collect();
    let mut sorted: Vec<usize> = (0..bands.len()).collect();
    sorted.sort_by_key(|&i| std::cmp::Reverse(sizes[i]));
    bands = sorted.iter().map(|&i| bands[i].clone()).collect();

    FlowModel {
        currency: flow.currency.to_string(),
        columns: columns.len(),
        nodes,
        bands,
        total: text.amount(flow.total),
        elsewhere: elsewhere
            .iter()
            .map(|other| text.amount(other.volume))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{CurrencyCode, Money};
    use kassensturz_report::{Flow, FlowNode, Percent};

    use super::*;

    struct Words;

    impl TextFormat for Words {
        fn amount(&self, money: Money) -> String {
            format!("{} {}", money.minor_units(), money.currency())
        }
        fn month(&self, first_day: kassensturz_core::Date) -> String {
            first_day.to_string()
        }
        fn percent(&self, _: Percent) -> String {
            String::new()
        }
        fn number(&self, hundredths: i64) -> String {
            hundredths.to_string()
        }
        fn unsorted(&self) -> String {
            "(unsorted)".into()
        }
    }

    fn eur() -> CurrencyCode {
        CurrencyCode::new("EUR").expect("EUR")
    }

    fn node(kind: FlowNodeKind, name: &str, column: i8, amount: i64) -> FlowNode {
        FlowNode {
            kind,
            name: name.into(),
            column,
            amount: Money::from_minor_units(amount, eur()),
            category: None,
            account: None,
            holds: Vec::new(),
        }
    }

    fn band(from: usize, to: usize, amount: i64) -> Flow {
        Flow {
            from,
            to,
            amount: Money::from_minor_units(amount, eur()),
        }
    }

    /// Salary 3000 in; rent 1000, food 500 and 1500 left over out.
    fn sample() -> MoneyFlow {
        MoneyFlow {
            currency: eur(),
            nodes: vec![
                node(FlowNodeKind::Middle, "", 0, 3_000),
                node(FlowNodeKind::Category, "Salary", -1, 3_000),
                node(FlowNodeKind::Category, "Food", 1, 500),
                node(FlowNodeKind::Category, "Rent", 1, 1_000),
                node(FlowNodeKind::LeftOver, "", 1, 1_500),
            ],
            flows: vec![
                band(1, 0, 3_000),
                band(0, 2, 500),
                band(0, 3, 1_000),
                band(0, 4, 1_500),
            ],
            total: Money::from_minor_units(3_000, eur()),
            volume: Money::from_minor_units(4_500, eur()),
        }
    }

    #[test]
    fn nodes_are_ordered_and_heights_follow_the_amounts() {
        let model = flow_model(&sample(), &[], &Words);
        assert_eq!(model.columns, 3);
        // Rent (largest) above Food, the balance node last.
        let y = |name: &str| model.nodes.iter().find(|n| n.name == name).unwrap().y;
        assert!(y("Rent") < y("Food"));
        let left = model
            .nodes
            .iter()
            .find(|n| n.kind == FlowNodeKind::LeftOver)
            .unwrap();
        assert!(left.y > y("Food"));
        // One scale: rent is twice as tall as food.
        let h = |name: &str| model.nodes.iter().find(|n| n.name == name).unwrap().height;
        assert!((h("Rent") - 2 * h("Food")).abs() <= 2);
        assert_eq!(model.nodes[0].label, "3000 EUR");
        assert_eq!(model.total, "3000 EUR");
    }

    #[test]
    fn bands_stay_inside_their_nodes_and_inside_the_chart() {
        let flow = sample();
        let model = flow_model(&flow, &[], &Words);
        for band in &model.bands {
            let (from, to) = (&model.nodes[band.from], &model.nodes[band.to]);
            let eps = 0;
            assert!(band.y_from >= from.y - eps);
            assert!(band.y_from + band.thickness <= from.y + from.height + eps);
            assert!(band.y_to >= to.y - eps);
            assert!(band.y_to + band.thickness <= to.y + to.height + eps);
            assert!(band.y_from >= 0 && band.y_to + band.thickness <= WHOLE);
        }
        // The middle node's bands fill it.
        let out: i64 = model
            .bands
            .iter()
            .filter(|b| b.from == 0)
            .map(|b| b.thickness)
            .sum();
        assert!((out - model.nodes[0].height).abs() <= 3);
        // Largest band first, and every band says its amount as text.
        assert_eq!(model.bands[0].label, "3000 EUR");
    }

    #[test]
    fn other_currencies_are_named_not_drawn() {
        let model = flow_model(&sample(), &[sample()], &Words);
        assert_eq!(model.elsewhere, vec!["4500 EUR".to_string()]);
    }
}
