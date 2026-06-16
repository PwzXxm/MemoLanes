//! The coverage primitive — the heart of the achievement system.
//!
//! Walk a pre-merged `JourneyBitmap` once, attribute each set block to a
//! geo entity via the `GeoLookupTable`, and measure the per-entity
//! covered area. `RegionFootprint::Bitmap` (POI) regions are measured by
//! direct bitmap intersection instead.
//!
//! This module measures area/coverage only; `first_visited` is supplied by
//! the caller (area-only callers pass `None`).

use std::collections::HashMap;

use crate::journey_area_utils::compute_journey_bitmap_area;
use crate::journey_bitmap::{Block, BlockKey, JourneyBitmap, Tile, TileKey, BITMAP_SIZE};

use super::geo_entity::GeoEntityId;
use super::geo_lookup::GeoLookupTable;
use super::region::{Coverage, NamedRegion, RegionFootprint, RegionId};

/// Per-entity covered area, computed by ONE full walk of a merged
/// bitmap: each attributed block is partitioned to its entity and the
/// per-entity bitmaps are measured once. The partition is independent of
/// which regions a query asks about, so build this once per query and
/// share it across every coverage-style composite (summary counts,
/// per-kind coverage, the whole entity-detail tree).
#[derive(Default)]
pub struct EntityAreas {
    areas: HashMap<GeoEntityId, u64>,
}

impl EntityAreas {
    pub fn covered_area_m2(&self, entity: GeoEntityId) -> u64 {
        self.areas.get(&entity).copied().unwrap_or(0)
    }
}

/// Build the [`EntityAreas`] partition: walk `bitmap` once, attribute
/// each block via `lookup`, and measure the per-entity area.
pub fn compute_entity_areas(bitmap: &JourneyBitmap, lookup: &GeoLookupTable) -> EntityAreas {
    let mut per_entity_bitmaps: HashMap<GeoEntityId, JourneyBitmap> = HashMap::new();
    let tile_keys: Vec<TileKey> = bitmap.all_tile_keys().copied().collect();
    for tile_pos in &tile_keys {
        let attributions: Vec<(GeoEntityId, BlockKey, Block)> = bitmap
            .peek_tile_without_updating_cache(tile_pos, |tile_opt| {
                let mut out = Vec::new();
                if let Some(tile) = tile_opt {
                    for (block_key, block) in tile.iter() {
                        let lookup_val =
                            lookup.lookup(tile_pos.x, tile_pos.y, block_key.x(), block_key.y());
                        if let Some(entity_id) = lookup_val {
                            out.push((entity_id, block_key, block.clone()));
                        }
                    }
                }
                out
            });
        for (entity_id, block_key, block) in attributions {
            let entity_bm = per_entity_bitmaps.entry(entity_id).or_default();
            let entity_tile = entity_bm.get_tile_mut_or_insert_empty(tile_pos);
            entity_tile.set(&block_key, block);
        }
    }
    EntityAreas {
        areas: per_entity_bitmaps
            .into_iter()
            .map(|(eid, bm)| (eid, compute_journey_bitmap_area(&bm, None)))
            .collect(),
    }
}

/// Coverage for `regions` from a prebuilt [`EntityAreas`] partition.
/// `bitmap` is still consulted for `RegionFootprint::Bitmap` (POI)
/// regions, which are not part of the geo partition.
pub fn coverage_with_areas(
    areas: &EntityAreas,
    bitmap: &JourneyBitmap,
    first_visited: Option<&HashMap<RegionId, chrono::NaiveDate>>,
    regions: &[NamedRegion],
) -> Vec<Coverage> {
    regions
        .iter()
        .map(|r| {
            let covered = match &r.footprint {
                RegionFootprint::GeoLookup(eid) => areas.covered_area_m2(*eid),
                RegionFootprint::Bitmap(bm) => intersect_area(bitmap, bm),
            };
            Coverage {
                region_id: r.id.clone(),
                covered_area_m2: covered,
                total_area_m2: r.total_area_m2,
                first_visited: first_visited.and_then(|fv| fv.get(&r.id).copied()),
            }
        })
        .collect()
}

/// Compute coverage for `regions` from a pre-merged `bitmap`.
///
/// `first_visited` is reflected verbatim into the returned
/// `Coverage::first_visited` field; pass `None` for area-only callers.
///
/// Each call walks the full bitmap once (when any geo region is
/// present). Callers issuing several coverage queries against the same
/// bitmap should build [`compute_entity_areas`] once and use
/// [`coverage_with_areas`] instead.
pub fn coverage(
    bitmap: &JourneyBitmap,
    first_visited: Option<&HashMap<RegionId, chrono::NaiveDate>>,
    regions: &[NamedRegion],
    lookup: &GeoLookupTable,
) -> Vec<Coverage> {
    let geo_region_present = regions
        .iter()
        .any(|r| matches!(r.footprint, RegionFootprint::GeoLookup(_)));
    let areas = if geo_region_present {
        compute_entity_areas(bitmap, lookup)
    } else {
        EntityAreas::default()
    };
    coverage_with_areas(&areas, bitmap, first_visited, regions)
}

/// Compute the area (m²) of `a ∩ b`.
fn intersect_area(a: &JourneyBitmap, b: &JourneyBitmap) -> u64 {
    let mut intersection = JourneyBitmap::new();
    let tile_keys: Vec<TileKey> = a.all_tile_keys().copied().collect();
    for tile_pos in &tile_keys {
        if !b.contains_tile(tile_pos) {
            continue;
        }
        // Collect intersected blocks for this tile pair, then assemble outside
        // the closures so we can mutate `intersection`.
        let int_blocks: Vec<(BlockKey, [u8; BITMAP_SIZE])> =
            a.peek_tile_without_updating_cache(tile_pos, |ta| {
                let Some(tile_a) = ta else { return Vec::new() };
                b.peek_tile_without_updating_cache(tile_pos, |tb| {
                    let Some(tile_b) = tb else { return Vec::new() };
                    let mut out: Vec<(BlockKey, [u8; BITMAP_SIZE])> = Vec::new();
                    for (block_key, block_a) in tile_a.iter() {
                        let Some(block_b) = tile_b.get(&block_key) else {
                            continue;
                        };
                        let a_data = block_a.raw_data();
                        let b_data = block_b.raw_data();
                        let mut int_data = [0u8; BITMAP_SIZE];
                        let mut block_any = false;
                        for ((dst, a_byte), b_byte) in
                            int_data.iter_mut().zip(a_data.iter()).zip(b_data.iter())
                        {
                            let v = a_byte & b_byte;
                            *dst = v;
                            if v != 0 {
                                block_any = true;
                            }
                        }
                        if block_any {
                            out.push((block_key, int_data));
                        }
                    }
                    out
                })
            });
        if !int_blocks.is_empty() {
            let mut int_tile = Tile::new();
            for (block_key, int_data) in int_blocks {
                int_tile.set(&block_key, Block::new_with_data(int_data));
            }
            intersection.insert_tile(tile_pos, int_tile);
        }
    }
    compute_journey_bitmap_area(&intersection, None)
}
