use std::path::Path;

use geo_data_format::GeoEntityKind;
use geo_rasterizer::entities::{assemble_entities, EntityModel};
use geo_rasterizer::parse::parse_geojson;
use geo_rasterizer::registry::Registry;

const SYNTHETIC_REGISTRY: &str = "tests/fixtures/synthetic_registry.toml";

#[test]
fn assemble_groups_continents_and_countries() {
    let features = parse_geojson(Path::new("tests/fixtures/synthetic.geojson")).unwrap();
    let registry = Registry::load(Path::new(SYNTHETIC_REGISTRY)).unwrap();
    let model: EntityModel = assemble_entities(&features, &registry).unwrap();

    // 3 distinct continents in synthetic: Europe, Asia, Africa.
    let continent_count = model
        .entities
        .iter()
        .filter(|e| matches!(e.kind, GeoEntityKind::Continent))
        .count();
    assert_eq!(continent_count, 3);

    // 3 country entities, one per ADM0_A3.
    let country_count = model
        .entities
        .iter()
        .filter(|e| matches!(e.kind, GeoEntityKind::Country))
        .count();
    assert_eq!(country_count, 3);
}

#[test]
fn assemble_collapses_duplicate_adm0_a3() {
    use geo_rasterizer::registry::Entry;
    use serde_json::json;
    let raw = json!({
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "properties": {"ADM0_A3":"FRA","ISO_A3":"-99","ISO_A3_EH":"FRA","NAME":"France","CONTINENT":"Europe","REGION_UN":"Europe","TYPE":"Country"},
                "geometry": {"type":"Polygon","coordinates":[[[2.0,48.0],[3.0,48.0],[3.0,49.0],[2.0,49.0],[2.0,48.0]]]}
            },
            {
                "type": "Feature",
                "properties": {"ADM0_A3":"FRA","ISO_A3":"GUF","ISO_A3_EH":"GUF","NAME":"French Guiana","CONTINENT":"South America","REGION_UN":"Americas","TYPE":"Geo unit"},
                "geometry": {"type":"Polygon","coordinates":[[[-53.0,4.0],[-52.0,4.0],[-52.0,5.0],[-53.0,5.0],[-53.0,4.0]]]}
            }
        ]
    });
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), serde_json::to_string(&raw).unwrap()).unwrap();
    let features = parse_geojson(tmp.path()).unwrap();
    // Build an inline registry: EU=0, SA=1, FRA=2.
    let registry = Registry {
        schema: 1,
        continents: vec![
            Entry {
                code: "EU".into(),
                id: 0,
                refs: std::collections::BTreeMap::from([("iso".to_string(), [2.5_f64, 48.5_f64])]),
            },
            Entry {
                code: "SA".into(),
                id: 1,
                refs: std::collections::BTreeMap::from([("iso".to_string(), [-52.5_f64, 4.5_f64])]),
            },
        ],
        countries: vec![Entry {
            code: "FRA".into(),
            id: 2,
            refs: std::collections::BTreeMap::from([("iso".to_string(), [2.5_f64, 48.5_f64])]),
        }],
    };
    let model = assemble_entities(&features, &registry).unwrap();
    let fra = model
        .entities
        .iter()
        .filter(|e| matches!(e.kind, GeoEntityKind::Country))
        .find(|e| e.canonical_code == "FRA")
        .expect("FRA should exist exactly once");
    // Collapsed group: the sovereign is the sole TYPE=="Country" member
    // (France → FRA), not the detached dependency (French Guiana, "Geo unit" → GUF).
    assert_eq!(fra.iso_a3_eh.as_deref(), Some("FRA"));
    // Its parent should be Europe (the metropole continent), not South America.
    let parent = model
        .entities
        .iter()
        .find(|e| Some(e.id) == fra.parent_id)
        .unwrap();
    assert_eq!(parent.canonical_code, "EU");
    // The collapsed FRA must own both polygons.
    let merged = model.geometry_for_country.get("FRA").unwrap();
    assert_eq!(merged.0.len(), 2);
}

/// A single-feature country IS the whole country, so its own ISO_A3_EH is
/// authoritative even when NE's ADM0_A3 is a non-ISO code and TYPE is not
/// "Country" (e.g. Palestine PSX → PSE, TYPE "Indeterminate").
#[test]
fn single_feature_uses_own_iso_a3_eh_even_when_adm0_differs() {
    use geo_rasterizer::registry::Entry;
    use serde_json::json;
    let raw = json!({
        "type": "FeatureCollection",
        "features": [{
            "type": "Feature",
            "properties": {"ADM0_A3":"PSX","ISO_A3":"PSE","ISO_A3_EH":"PSE","NAME":"Palestine","CONTINENT":"Asia","REGION_UN":"Asia","TYPE":"Indeterminate"},
            "geometry": {"type":"Polygon","coordinates":[[[35.0,31.0],[36.0,31.0],[36.0,32.0],[35.0,32.0],[35.0,31.0]]]}
        }]
    });
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), serde_json::to_string(&raw).unwrap()).unwrap();
    let features = parse_geojson(tmp.path()).unwrap();
    let registry = Registry {
        schema: 1,
        continents: vec![Entry {
            code: "AS".into(),
            id: 0,
            refs: std::collections::BTreeMap::from([("iso".to_string(), [35.5_f64, 31.5_f64])]),
        }],
        countries: vec![Entry {
            code: "PSX".into(),
            id: 1,
            refs: std::collections::BTreeMap::from([("iso".to_string(), [35.5_f64, 31.5_f64])]),
        }],
    };
    let model = assemble_entities(&features, &registry).unwrap();
    let psx = model
        .entities
        .iter()
        .find(|e| e.canonical_code == "PSX")
        .expect("PSX entity");
    assert_eq!(psx.iso_a3_eh.as_deref(), Some("PSE"));
}

/// A collapsed group with no TYPE=="Country" member has no single sovereign ISO
/// (the NE `IOA` bucket = Cocos + Christmas, two distinct ISO territories).
#[test]
fn collapsed_group_without_sovereign_member_is_none() {
    use geo_rasterizer::registry::Entry;
    use serde_json::json;
    let raw = json!({
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "properties": {"ADM0_A3":"IOA","ISO_A3":"CCK","ISO_A3_EH":"CCK","NAME":"Cocos Is.","CONTINENT":"Oceania","REGION_UN":"Oceania","TYPE":"Geo unit"},
                "geometry": {"type":"Polygon","coordinates":[[[96.0,-12.0],[97.0,-12.0],[97.0,-11.0],[96.0,-11.0],[96.0,-12.0]]]}
            },
            {
                "type": "Feature",
                "properties": {"ADM0_A3":"IOA","ISO_A3":"CXR","ISO_A3_EH":"CXR","NAME":"Christmas I.","CONTINENT":"Oceania","REGION_UN":"Oceania","TYPE":"Geo unit"},
                "geometry": {"type":"Polygon","coordinates":[[[105.0,-11.0],[106.0,-11.0],[106.0,-10.0],[105.0,-10.0],[105.0,-11.0]]]}
            }
        ]
    });
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), serde_json::to_string(&raw).unwrap()).unwrap();
    let features = parse_geojson(tmp.path()).unwrap();
    let registry = Registry {
        schema: 1,
        continents: vec![Entry {
            code: "OC".into(),
            id: 0,
            refs: std::collections::BTreeMap::from([("iso".to_string(), [100.0_f64, -11.0_f64])]),
        }],
        countries: vec![Entry {
            code: "IOA".into(),
            id: 1,
            refs: std::collections::BTreeMap::from([("iso".to_string(), [100.0_f64, -11.0_f64])]),
        }],
    };
    let model = assemble_entities(&features, &registry).unwrap();
    let ioa = model
        .entities
        .iter()
        .find(|e| e.canonical_code == "IOA")
        .expect("IOA entity");
    assert_eq!(ioa.iso_a3_eh, None);
}

#[test]
fn entity_ids_are_dense_and_continents_first() {
    let features = parse_geojson(Path::new("tests/fixtures/synthetic.geojson")).unwrap();
    let registry = Registry::load(Path::new(SYNTHETIC_REGISTRY)).unwrap();
    let model = assemble_entities(&features, &registry).unwrap();
    // Continents at IDs 0..continent_count; countries follow (registry assigns 0-2 to
    // continents and 3-5 to countries).
    let mut ids: Vec<u32> = model.entities.iter().map(|e| e.id.0).collect();
    ids.sort();
    assert_eq!(ids, (0..ids.len() as u32).collect::<Vec<_>>());
    let last_continent_id = model
        .entities
        .iter()
        .filter(|e| matches!(e.kind, GeoEntityKind::Continent))
        .map(|e| e.id.0)
        .max()
        .unwrap();
    let first_country_id = model
        .entities
        .iter()
        .filter(|e| matches!(e.kind, GeoEntityKind::Country))
        .map(|e| e.id.0)
        .min()
        .unwrap();
    assert!(first_country_id > last_continent_id);
}

#[test]
fn unused_lookup_value_is_referenced() {
    let features = parse_geojson(Path::new("tests/fixtures/synthetic.geojson")).unwrap();
    let registry = Registry::load(Path::new(SYNTHETIC_REGISTRY)).unwrap();
    let model = assemble_entities(&features, &registry).unwrap();
    let _value = model.entities[0].id;
}
