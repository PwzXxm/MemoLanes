use std::collections::BTreeMap;

use chrono::NaiveDate;
use geo_data_format::{
    tile_index, write_geo_data, GeoEntity, GeoEntityId, GeoEntityKind, TileMembership,
    CELLS_PER_TILE, TILE_COUNT,
};
use memolanes_core::{
    achievement::region_index::{compute_region_states, JourneyFootprint},
    achievement::scope::AchievementLayer,
    geo::GeoIndex,
    journey_area_utils::compute_journey_bitmap_area,
    journey_bitmap::{Block, BlockKey, JourneyBitmap, TileKey},
    journey_header::JourneyKind,
};

const FR: GeoEntityId = GeoEntityId(2);
const DE: GeoEntityId = GeoEntityId(3);
const EU: GeoEntityId = GeoEntityId(1);

fn entity(id: u32, kind: GeoEntityKind, iso: &str, parent: Option<u32>, area: u64) -> GeoEntity {
    GeoEntity {
        id: GeoEntityId(id),
        kind,
        iso_code: iso.into(),
        name_key: format!("k.{iso}"),
        parent_id: parent.map(GeoEntityId),
        total_area_m2: area,
    }
}

/// Geo where the whole tile (0,0) is France and tile (1,0) is a border tile
/// owned block-by-block by Germany. EU is their parent continent. Entity total
/// areas are deliberately tiny so a single visited block completes them.
fn synthetic_geo() -> GeoIndex {
    let entities = [
        entity(1, GeoEntityKind::Continent, "EU", None, 1),
        entity(2, GeoEntityKind::Country, "FR", Some(1), 1),
        entity(3, GeoEntityKind::Country, "DE", Some(1), 1),
    ];
    let mut tiles = vec![TileMembership::None; TILE_COUNT];
    tiles[tile_index(0, 0)] = TileMembership::Single(FR);
    tiles[tile_index(1, 0)] = TileMembership::Border;
    let mut cells = vec![None; CELLS_PER_TILE];
    cells[BlockKey::from_x_y(7, 7).index()] = Some(DE);
    let mut blocks: BTreeMap<(u16, u16), Vec<Option<GeoEntityId>>> = BTreeMap::new();
    blocks.insert((1, 0), cells);
    let bytes = write_geo_data(&entities, &[], &tiles, &blocks, [0u8; 32]).unwrap();
    GeoIndex::from_bytes(&bytes).unwrap()
}

/// A bitmap with `bits` pixels set in one block of one tile.
fn one_block(tile: TileKey, block: BlockKey, bits: u32) -> JourneyBitmap {
    let mut bm = JourneyBitmap::new();
    let mut b = Block::new();
    for i in 0..bits {
        b.set_point((i % 64) as u8, (i / 64) as u8, true);
    }
    bm.get_tile_mut_or_insert_empty(&tile).set(&block, b);
    bm
}

fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 1, day).unwrap()
}

#[test]
fn attributes_area_with_rollup_and_layers() {
    let geo = synthetic_geo();

    // Day 1: a Default journey in France (tile 0,0). Day 2: a Flight journey
    // over Germany (border tile 1,0). Both blocks carry the same bit pattern.
    let fr_block = one_block(TileKey::new(0, 0), BlockKey::from_x_y(3, 4), 20);
    let de_block = one_block(TileKey::new(1, 0), BlockKey::from_x_y(7, 7), 20);
    let fr_area = compute_journey_bitmap_area(&fr_block, None);
    let de_area = compute_journey_bitmap_area(&de_block, None);
    // EU's area is the union (disjoint blocks), summed as f64 then rounded —
    // not round(fr)+round(de), which can differ by a metre from rounding.
    let mut union = fr_block.clone();
    union.merge(de_block.clone());
    let eu_area = compute_journey_bitmap_area(&union, None);

    let states = compute_region_states(
        [
            JourneyFootprint {
                date: d(1),
                kind: JourneyKind::DefaultKind,
                bitmap: fr_block,
            },
            JourneyFootprint {
                date: d(2),
                kind: JourneyKind::Flight,
                bitmap: de_block,
            },
        ],
        &geo,
    );

    // France: Default layer only, area == the block oracle, completed on day 1.
    let fr = &states[&(AchievementLayer::Default, FR)];
    assert_eq!(fr.visited_area_m2, fr_area);
    assert_eq!(fr.first_visit_date, d(1));
    assert_eq!(fr.completed_at, Some(d(1)));
    assert!(!states.contains_key(&(AchievementLayer::Flight, FR)));

    // Germany: Flight layer only, first visited day 2.
    let de = &states[&(AchievementLayer::Flight, DE)];
    assert_eq!(de.visited_area_m2, de_area);
    assert_eq!(de.first_visit_date, d(2));
    assert!(!states.contains_key(&(AchievementLayer::Default, DE)));

    // EU rollup: All layer is the union of both kinds, earliest first visit.
    let eu_all = &states[&(AchievementLayer::All, EU)];
    assert_eq!(eu_all.visited_area_m2, eu_area);
    assert_eq!(eu_all.first_visit_date, d(1));
    // Per-layer EU: Default sees only FR, Flight only DE.
    assert_eq!(
        states[&(AchievementLayer::Default, EU)].visited_area_m2,
        fr_area
    );
    assert_eq!(
        states[&(AchievementLayer::Flight, EU)].visited_area_m2,
        de_area
    );
}

#[test]
fn dedups_within_layer_across_overlapping_journeys() {
    let geo = synthetic_geo();

    // Same France block visited twice; first visit keeps the earliest date and
    // the area is not double counted within the Default layer.
    let area = compute_journey_bitmap_area(
        &one_block(TileKey::new(0, 0), BlockKey::from_x_y(1, 1), 30),
        None,
    );
    let states = compute_region_states(
        [
            JourneyFootprint {
                date: d(5),
                kind: JourneyKind::DefaultKind,
                bitmap: one_block(TileKey::new(0, 0), BlockKey::from_x_y(1, 1), 30),
            },
            JourneyFootprint {
                date: d(9),
                kind: JourneyKind::DefaultKind,
                bitmap: one_block(TileKey::new(0, 0), BlockKey::from_x_y(1, 1), 30),
            },
        ],
        &geo,
    );

    let fr = &states[&(AchievementLayer::Default, FR)];
    assert_eq!(fr.visited_area_m2, area);
    assert_eq!(fr.first_visit_date, d(5));
}

#[test]
fn ocean_blocks_are_ignored() {
    let geo = synthetic_geo();
    // Border tile (1,0), but a block with no geo owner.
    let states = compute_region_states(
        [JourneyFootprint {
            date: d(1),
            kind: JourneyKind::DefaultKind,
            bitmap: one_block(TileKey::new(1, 0), BlockKey::from_x_y(0, 0), 10),
        }],
        &geo,
    );
    assert!(states.is_empty());
}
