//! End-to-end: load the real `app/assets/geo_data_iso.bin`, draw a small
//! journey over a known country, and assert the countries-visited
//! composite reports that country (and not an untouched one).

use std::path::PathBuf;

use memolanes_core::achievement::composites::visited_countries;
use memolanes_core::achievement::geo_lookup::GeoLookupTable;
use memolanes_core::achievement::region::RegionId;
use memolanes_core::journey_bitmap::JourneyBitmap;

fn load_table() -> GeoLookupTable {
    let path: PathBuf = std::env::var("GEO_DATA_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // CARGO_MANIFEST_DIR is `app/rust`; bin is at `../assets/geo_data_iso.bin`.
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("assets")
                .join("geo_data_iso.bin")
        });
    let bytes = std::fs::read(&path).unwrap_or_else(|e| {
        panic!("failed to read {}: {e}", path.display());
    });
    GeoLookupTable::load_from_bytes(&bytes).expect("geo_data_iso.bin should load")
}

/// The iso codes of every visited country region in `bitmap`.
fn visited_iso_codes(bitmap: &JourneyBitmap, table: &GeoLookupTable) -> Vec<String> {
    visited_countries(bitmap, None, table)
        .iter()
        .filter_map(|c| match &c.region_id {
            RegionId::GeoEntity(id) => table.get_entity(*id).map(|e| e.iso_code.clone()),
            RegionId::Poi { .. } => None,
        })
        .collect()
}

#[test]
fn visited_countries_includes_touched_and_excludes_untouched() {
    let table = load_table();

    // A short journey through central Paris — solidly inside FRA.
    let mut bitmap = JourneyBitmap::new();
    bitmap.add_line(2.2945, 48.8584, 2.3522, 48.8700);

    let isos = visited_iso_codes(&bitmap, &table);

    assert!(
        isos.iter().any(|c| c == "FRA"),
        "a journey through Paris should mark FRA visited, got {isos:?}"
    );
    assert!(
        !isos.iter().any(|c| c == "JPN"),
        "an untouched country (JPN) must not be visited, got {isos:?}"
    );
}
