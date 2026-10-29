// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `ChartProvider` boundary (D-276): the one place exact amounts become
//! drawing values.

use crate::{ChartError, ChartKind, ChartModel};

/// The largest amount in minor units a drawing value holds exactly: 2^53.
const EXACT_LIMIT: u64 = 1 << 53;

/// An exact amount as a drawing value.
///
/// Drawing values place marks and nothing else: they are never read back,
/// never added up and never shown (AGENTS.md §5). An amount of more than
/// 2^53 minor units would lose digits as a `f64`, so it is refused.
pub fn drawing_value(minor_units: i64) -> Result<f64, ChartError> {
    if minor_units.unsigned_abs() > EXACT_LIMIT {
        return Err(ChartError::ValueTooLarge);
    }
    // Exact: the magnitude is at most 2^53, which `f64` holds without
    // rounding.
    #[expect(clippy::cast_precision_loss, reason = "at most 2^53, checked above")]
    Ok(minor_units as f64)
}

/// A series with its drawing values.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSeries {
    /// One value per category, in minor units of the currency. A round
    /// chart gets 0 for a slice that cannot be shown (money that came
    /// back), so the indices still match the model.
    pub values: Vec<f64>,
}

/// What a provider is given to draw: the model's amounts as drawing values.
/// Labels, names and the table stay in the model; the provider asks the model
/// for the text of the point at an index.
#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    /// One entry per series of the model.
    pub series: Vec<PreparedSeries>,
    /// The ticks of the value axis as drawing values.
    pub ticks: Vec<f64>,
}

/// A library that draws charts: KQuickCharts in the app (D-275), a fake in
/// tests. It draws what the model says and formats no number.
pub trait ChartProvider {
    /// The provider's fixed name, e.g. `kquickcharts`.
    fn name(&self) -> &'static str;

    /// Whether it can draw this kind of chart.
    fn supports(&self, kind: ChartKind) -> bool;

    /// The model's amounts as drawing values, or an error when one would
    /// lose digits.
    fn prepare(&self, model: &ChartModel) -> Result<Prepared, ChartError> {
        let series = model
            .series
            .iter()
            .map(|series| {
                let values = series
                    .points
                    .iter()
                    .map(|point| {
                        // A slice without a size is not drawn.
                        if model.kind.is_round() && point.minor_units <= 0 {
                            Ok(0.0)
                        } else {
                            drawing_value(point.minor_units)
                        }
                    })
                    .collect::<Result<_, _>>()?;
                Ok(PreparedSeries { values })
            })
            .collect::<Result<_, ChartError>>()?;
        let ticks = model
            .ticks
            .iter()
            .map(|tick| drawing_value(tick.minor_units))
            .collect::<Result<_, _>>()?;
        Ok(Prepared { series, ticks })
    }
}
