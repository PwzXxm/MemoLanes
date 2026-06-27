//! Region read-model: per-`(layer, entity)` visited area, first-visit date, and
//! completion, by intersecting journey footprints with the geo lookup. Pure.

use std::collections::HashMap;

use chrono::NaiveDate;
use geo_data_format::GeoEntityId;

use crate::achievement::scope::AchievementLayer;
use crate::geo::GeoLookup;
use crate::journey_area_utils::block_area_m2;
use crate::journey_bitmap::JourneyBitmap;
use crate::journey_header::JourneyKind;

/// Area fraction counting as "fully explored"; below 1.0 to tolerate
/// rasterization gaps between geo cells and the journey grid.
pub const COMPLETION_THRESHOLD: f64 = 0.95;

/// Per-`(layer, entity)` region coverage.
pub type RegionStateMap = HashMap<(AchievementLayer, GeoEntityId), RegionState>;

/// One entity's coverage within one layer.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionState {
    /// Earliest journey (of this layer) that added a new block to this entity.
    pub first_visit_date: NaiveDate,
    /// Latitude-corrected area covered, deduped within this layer.
    pub visited_area_m2: u64,
    /// When this layer's coverage first crossed [`COMPLETION_THRESHOLD`].
    pub completed_at: Option<NaiveDate>,
}

/// One journey's contribution to the region index: its day (a journey spans a
/// single day), kind, and block footprint.
pub struct JourneyFootprint {
    pub date: NaiveDate,
    pub kind: JourneyKind,
    pub bitmap: JourneyBitmap,
}

struct Accum {
    first_visit_date: NaiveDate,
    area_m2: f64,
    completed_at: Option<NaiveDate>,
}

/// Compute the region states from journeys **in chronological order**
/// (`journey_date`, then tie-breakers — the order `query_journeys` returns).
///
/// Per-layer attribution: each journey feeds the layers of its kind
/// (`Default`→{Default,All}, `Flight`→{Flight,All}); a per-layer `seen` bitmap
/// dedups area so `All` is the true union, never `Default + Flight`. Dates are
/// read from each journey, never `now()`, so a recompute is deterministic.
pub fn compute_region_states(
    journeys: impl IntoIterator<Item = JourneyFootprint>,
    geo: &dyn GeoLookup,
) -> RegionStateMap {
    let mut states: HashMap<(AchievementLayer, GeoEntityId), Accum> = HashMap::new();
    let mut seen: HashMap<AchievementLayer, JourneyBitmap> = AchievementLayer::ALL_LAYERS
        .into_iter()
        .map(|l| (l, JourneyBitmap::new()))
        .collect();

    for JourneyFootprint { date, kind, bitmap } in journeys {
        for layer in AchievementLayer::layers_including(kind) {
            // Blocks new to THIS layer.
            let mut fresh = bitmap.clone();
            fresh.difference(&seen[&layer]);

            // Area new to each entity (and its ancestors) this journey.
            let mut touched: HashMap<GeoEntityId, f64> = HashMap::new();
            let tile_keys: Vec<_> = fresh.all_tile_keys().cloned().collect();
            for tile_key in &tile_keys {
                fresh.peek_tile_without_updating_cache(tile_key, |tile| {
                    if let Some(tile) = tile {
                        for (block_key, block) in tile.iter() {
                            let bit_count = block.count();
                            if bit_count == 0 {
                                continue;
                            }
                            if let Some(entity) = geo.entity_of_block(*tile_key, block_key) {
                                let area = block_area_m2(tile_key, &block_key, bit_count);
                                *touched.entry(entity).or_default() += area;
                                for ancestor in geo.ancestors(entity) {
                                    *touched.entry(ancestor).or_default() += area;
                                }
                            }
                        }
                    }
                });
            }

            for (entity, area) in touched {
                let state = states.entry((layer, entity)).or_insert(Accum {
                    first_visit_date: date, // first writer wins → true earliest day
                    area_m2: 0.0,
                    completed_at: None,
                });
                state.area_m2 += area;
                if state.completed_at.is_none() {
                    if let Some(total) = geo.entity(entity).map(|e| e.total_area_m2) {
                        if total > 0 && state.area_m2 / total as f64 >= COMPLETION_THRESHOLD {
                            state.completed_at = Some(date);
                        }
                    }
                }
            }

            seen.get_mut(&layer).unwrap().merge(bitmap.clone());
        }
    }

    states
        .into_iter()
        .map(|(key, a)| {
            (
                key,
                RegionState {
                    first_visit_date: a.first_visit_date,
                    visited_area_m2: a.area_m2.round() as u64,
                    completed_at: a.completed_at,
                },
            )
        })
        .collect()
}
