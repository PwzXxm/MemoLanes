//! Coverage composites — thin shaping layers over the `coverage`
//! primitive. The FRB-exposed functions in `api/achievement.rs` call
//! these and pack the results into the public wire types; they do not
//! compute anything themselves.

use std::collections::HashMap;

use chrono::NaiveDate;

use super::coverage::coverage;
use super::geo_entity::GeoEntityKind;
use super::geo_lookup::GeoLookupTable;
use super::region::{geo_regions_of_kind, Coverage, RegionId};
use crate::journey_bitmap::JourneyBitmap;

/// Shared body of the kind-parameterized coverage composites: coverage
/// over all regions of one `GeoEntityKind`.
fn coverage_of_kind(
    bitmap: &JourneyBitmap,
    first_visited: Option<&HashMap<RegionId, NaiveDate>>,
    lookup: &GeoLookupTable,
    kind: GeoEntityKind,
) -> Vec<Coverage> {
    let regions = geo_regions_of_kind(lookup, kind);
    coverage(bitmap, first_visited, &regions, lookup)
}

/// `coverage_of_kind` filtered to visited regions.
fn visited_of_kind(
    bitmap: &JourneyBitmap,
    first_visited: Option<&HashMap<RegionId, NaiveDate>>,
    lookup: &GeoLookupTable,
    kind: GeoEntityKind,
) -> Vec<Coverage> {
    coverage_of_kind(bitmap, first_visited, lookup, kind)
        .into_iter()
        .filter(|c| c.visited())
        .collect()
}

/// Countries with any explored area in the given merged `bitmap`.
pub fn visited_countries(
    bitmap: &JourneyBitmap,
    first_visited: Option<&HashMap<RegionId, NaiveDate>>,
    lookup: &GeoLookupTable,
) -> Vec<Coverage> {
    visited_of_kind(bitmap, first_visited, lookup, GeoEntityKind::Country)
}
