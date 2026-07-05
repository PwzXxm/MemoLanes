use std::collections::HashMap;

use anyhow::Result;

use flutter_rust_bridge::frb;
use geo_data_format::WorldviewVariant;

pub use crate::achievement::layer::AchievementLayer;
use crate::achievement::read_model::region;
pub use crate::achievement::read_model::region::{
    LevelSummary, RegionDetail, RegionEntity, RegionKind, RegionLevelView,
};
pub use geo_data_format::GeoEntityId;

// `GeoEntityId` (external `geo_data_format`) keys `RegionLevelView.entries` /
// `RegionDetail.children`. Mirror its field so Dart gets a value class with
// `==`/`hashCode` (usable as a `Map` key, buildable from an int), not an opaque box.
#[frb(mirror(GeoEntityId))]
pub struct _GeoEntityId(pub u32);

#[frb(mirror(WorldviewVariant))]
pub enum _WorldviewVariant {
    Iso,
    Chn,
    Usa,
}

// TODO: change these to a method instead of a function.
#[frb(sync)]
pub fn worldview_asset_path(worldview: &WorldviewVariant) -> String {
    format!("assets/geo/geo_data_{}.bin", worldview.spec().id)
}

#[frb(sync)]
pub fn worldview_of_string_opt(str: &str) -> Option<WorldviewVariant> {
    WorldviewVariant::from_id(str).ok()
}

#[frb(sync)]
pub fn worldview_to_string(worldview: &WorldviewVariant) -> &'static str {
    worldview.spec().id
}

#[frb(sync)]
pub fn default_worldview() -> WorldviewVariant {
    WorldviewVariant::ALL[0]
}

pub fn init_or_change_geo_data(worldview: WorldviewVariant, geo_data: &[u8]) -> Result<()> {
    crate::api::api::get()
        .storage
        .init_or_change_geo_data(worldview, geo_data)
}

/// Explored area for a single layer.
pub fn get_explored_area(layer: AchievementLayer) -> Result<u64> {
    crate::api::api::get()
        .storage
        .with_achievement_read(|store| store.explored_area_m2(layer))
}

/// Explored area for every layer, read under one snapshot so they can't skew.
pub fn get_explored_area_by_layer() -> Result<HashMap<AchievementLayer, u64>> {
    crate::api::api::get()
        .storage
        .with_achievement_read(|store| {
            AchievementLayer::ALL_LAYERS
                .into_iter()
                .map(|layer| Ok((layer, store.explored_area_m2(layer)?)))
                .collect()
        })
}

// Regions (layered): Flutter Rust Bridge entry points over `achievement::read_model::region`.

pub fn region_levels() -> Result<HashMap<RegionKind, LevelSummary>> {
    crate::api::api::get()
        .storage
        .with_achievement_read(|store| {
            Ok(store.geo().map_or_else(HashMap::new, region::region_levels))
        })
}

pub fn region_level_view(
    layer: AchievementLayer,
    level: RegionKind,
    parent: Option<GeoEntityId>,
) -> Result<RegionLevelView> {
    crate::api::api::get()
        .storage
        .with_achievement_read(|store| {
            let Some(geo) = store.geo() else {
                return Ok(RegionLevelView {
                    level,
                    visited_count: 0,
                    region_count: 0,
                    entries: HashMap::new(),
                });
            };
            Ok(region::region_level_view(
                &store.region_states(&[layer])?,
                geo,
                layer,
                level,
                parent,
            ))
        })
}

pub fn region_detail(
    entity_id: GeoEntityId,
    layer: AchievementLayer,
) -> Result<Option<RegionDetail>> {
    crate::api::api::get()
        .storage
        .with_achievement_read(|store| {
            let Some(geo) = store.geo() else {
                return Ok(None);
            };
            Ok(region::region_detail(
                &store.region_states(&[layer])?,
                geo,
                entity_id,
                layer,
            ))
        })
}
