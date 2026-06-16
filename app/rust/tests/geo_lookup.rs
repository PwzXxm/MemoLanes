//! Round-trips a tiny synthetic geo-data binary through
//! `GeoLookupTable::load_from_bytes` and checks the provenance hash plus
//! single- and border-tile resolution.

use std::collections::BTreeMap;

use geo_data_format::{
    write_geo_data, GeoEntity, GeoEntityId, GeoEntityKind, TileMembership, Worldview,
    CELLS_PER_TILE,
};
use memolanes_core::achievement::geo_lookup::GeoLookupTable;
use memolanes_core::journey_bitmap::MAP_WIDTH;

/// Build the smallest valid geo-data binary understood by
/// `GeoLookupTable::load_from_bytes`. `hash` is baked into the header so
/// callers can distinguish two distinct blobs.
fn tiny_geo_bin(hash: [u8; 32]) -> Vec<u8> {
    let mut tile_lookup = vec![TileMembership::None; (MAP_WIDTH * MAP_WIDTH) as usize];
    tile_lookup[0] = TileMembership::Single(GeoEntityId(0));
    tile_lookup[1] = TileMembership::Border;
    let mut cells = vec![None; CELLS_PER_TILE];
    cells[0] = Some(GeoEntityId(0));
    let mut block_lookup: BTreeMap<(u16, u16), Vec<Option<GeoEntityId>>> = BTreeMap::new();
    block_lookup.insert((1, 0), cells);
    let entities = vec![GeoEntity {
        id: GeoEntityId(0),
        kind: GeoEntityKind::Country,
        iso_code: "TEST".to_string(),
        name_key: "country.TEST.name".to_string(),
        parent_id: None,
        total_area_m2: 1_000_000,
    }];
    let worldviews = vec![Worldview {
        id: "iso".to_string(),
        name_key: "wv".to_string(),
        description_key: "wv".to_string(),
    }];
    write_geo_data(&entities, &worldviews, &tile_lookup, &block_lookup, hash).expect("write ok")
}

#[test]
fn empty_for_test_uses_zero_hash() {
    let t = GeoLookupTable::empty_for_test();
    assert_eq!(t.provenance_hash(), [0u8; 32]);
}

#[test]
fn load_from_bytes_round_trips() {
    let bytes = tiny_geo_bin([9u8; 32]);
    let table = GeoLookupTable::load_from_bytes(&bytes).expect("should load");
    assert_eq!(table.provenance_hash(), [9u8; 32]);
    // Single tile.
    assert_eq!(table.lookup(0, 0, 0, 0), Some(GeoEntityId(0)));
    // Border tile (tx=1, ty=0), cell 0 → resolves via BorderStore.
    assert_eq!(table.lookup(1, 0, 0, 0), Some(GeoEntityId(0)));
    // Border tile, an unset cell → None.
    assert_eq!(table.lookup(1, 0, 1, 0), None);
}

#[test]
fn load_from_bytes_rejects_bad_magic() {
    let mut bytes = tiny_geo_bin([0u8; 32]);
    bytes[0] = b'X';
    assert!(GeoLookupTable::load_from_bytes(&bytes).is_err());
}
