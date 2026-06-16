use anyhow::Result;
use flutter_rust_bridge::frb;

use crate::achievement::geo_entity::{GeoEntity, GeoEntityKind};
use crate::achievement::geo_lookup::GeoLookupTable;
use crate::achievement::region::RegionId;
pub use crate::achievement::scope::AchievementLayer;
use crate::journey_snapshot::JourneySnapshot;

/// Total explored area for one layer in a single struct, for the
/// "Default vs Flight vs All" comparison row.
#[frb(non_opaque)]
#[derive(Debug, Clone)]
pub struct LayerArea {
    pub layer: AchievementLayer,
    pub area_m2: u64,
}

/// Total explored area (m²) for one layer, from its merged finalized
/// bitmap. Shared by the single-layer and per-layer queries so they
/// always agree.
fn layer_area_m2(snapshot: &JourneySnapshot, layer: AchievementLayer) -> Result<u64> {
    let bitmap = snapshot.finalized_bitmap(&layer.to_layer_kind(), None)?;
    Ok(crate::journey_area_utils::compute_journey_bitmap_area(
        &bitmap, None,
    ))
}

/// Total explored area for a single layer: one merged-bitmap fetch + an
/// area sum (no first_visited or worldview needed).
pub fn get_explored_area(layer: AchievementLayer) -> Result<u64> {
    let storage = &crate::api::api::get().storage;
    storage.with_journey_snapshot(|snapshot| layer_area_m2(snapshot, layer))
}

/// Total explored area per layer, read as ONE consistent snapshot so the
/// rows can never contradict each other (e.g. `All` < `Default`). For the
/// Default-vs-Flight-vs-All comparison; for a single layer use
/// [`get_explored_area`].
pub fn get_explored_area_by_layer() -> Result<Vec<LayerArea>> {
    let storage = &crate::api::api::get().storage;
    storage.with_journey_snapshot(|snapshot| {
        AchievementLayer::ALL_LAYERS
            .into_iter()
            .map(|layer| {
                Ok(LayerArea {
                    layer,
                    area_m2: layer_area_m2(snapshot, layer)?,
                })
            })
            .collect()
    })
}

// ============================================================
// Worldviews (POV)
// ============================================================

const DEFAULT_WORLDVIEW_ID: &str = geo_data_format::Pov::Iso.spec().id;

fn resolve_startup_id(persisted: Option<&str>) -> String {
    match persisted {
        // `Pov::from_id` is the canonical shipped-id validator, derived from
        // `Pov::ALL`, so the accepted set can't drift from the shipped POVs.
        Some(id) if geo_data_format::Pov::from_id(id).is_ok() => id.to_string(),
        _ => DEFAULT_WORLDVIEW_ID.to_string(),
    }
}

/// Validate, persist, then publish the worldview — a thin shim over
/// `AchievementManager::install_active`, shared by `init_achievement_system`
/// and `switch_worldview`.
fn install_active(geo_data_bytes: &[u8], worldview_id: &str) -> Result<()> {
    let state = crate::api::api::get();
    state
        .achievement
        .install_active(&state.storage, worldview_id, geo_data_bytes)
}

#[frb(non_opaque)]
#[derive(Debug, Clone)]
pub struct WorldviewInfo {
    pub id: String,
    pub name_key: String,
    pub description_key: String,
}

/// List available worldviews of the active POV's bin.
pub fn get_worldviews() -> Result<Vec<WorldviewInfo>> {
    let active = crate::api::api::get()
        .achievement
        .active()
        .ok_or_else(|| anyhow::anyhow!("achievement system not initialized"))?;
    Ok(active
        .lookup
        .worldviews()
        .iter()
        .map(|w| WorldviewInfo {
            id: w.id.clone(),
            name_key: w.name_key.clone(),
            description_key: w.description_key.clone(),
        })
        .collect())
}

/// Switch the active worldview. Dart loads `assets/geo_data_<id>.bin` and
/// passes its bytes; reloads the lookup and republishes the active context.
pub fn switch_worldview(worldview_id: String, geo_data_bytes: Vec<u8>) -> Result<()> {
    install_active(&geo_data_bytes, &worldview_id)
}

/// Initialize the achievement system at app bootstrap (after `api::init`).
/// `worldview_id` is the POV whose bin `geo_data_bytes` contains (Dart
/// resolved it via `startup_worldview_id`). Idempotent / re-entrant: a
/// repeat call is equivalent to `switch_worldview`.
pub fn init_achievement_system(worldview_id: String, geo_data_bytes: Vec<u8>) -> Result<()> {
    install_active(&geo_data_bytes, &worldview_id)
}

/// The POV id Dart should load at startup: the persisted choice if it is a
/// shipped POV, else the default.
///
/// A failing settings read errors out rather than defaulting — defaulting
/// would let `init_achievement_system` persist the default over the user's
/// real choice.
pub fn startup_worldview_id() -> Result<String> {
    let persisted = crate::api::api::get().storage.get_active_worldview()?;
    Ok(resolve_startup_id(persisted.as_deref()))
}

// ============================================================
// Geo entities (catalog) + countries visited
// ============================================================

/// Self-describing geo-entity row: the numeric `GeoEntityId` never crosses
/// FRB — `iso_code` is the wire key, and each row embeds enough to render
/// without joining the catalog.
#[frb(non_opaque)]
#[derive(Debug, Clone)]
pub struct GeoEntityInfo {
    /// Stable, human-meaningful wire key (worldview-independent).
    pub iso_code: String,
    /// i18n key; the frontend resolves it via its locale bundles.
    pub name_key: String,
    /// One of: "continent" | "country" | "province" | "city".
    pub kind: String,
    pub parent_iso_code: Option<String>,
    pub total_area_m2: u64,
}

/// One of "continent" | "country" | "province" | "city".
fn kind_str(kind: GeoEntityKind) -> &'static str {
    match kind {
        GeoEntityKind::Continent => "continent",
        GeoEntityKind::Country => "country",
        GeoEntityKind::Province => "province",
        GeoEntityKind::City => "city",
    }
}

fn parse_kind(s: &str) -> Result<GeoEntityKind> {
    match s {
        "continent" => Ok(GeoEntityKind::Continent),
        "country" => Ok(GeoEntityKind::Country),
        "province" => Ok(GeoEntityKind::Province),
        "city" => Ok(GeoEntityKind::City),
        other => Err(anyhow::anyhow!("unknown geo entity kind `{other}`")),
    }
}

/// Map a `GeoEntity` to its wire form, resolving its parent's `iso_code`
/// via the lookup (the numeric parent id never crosses FRB).
fn entity_info(e: &GeoEntity, lookup: &GeoLookupTable) -> GeoEntityInfo {
    GeoEntityInfo {
        iso_code: e.iso_code.clone(),
        name_key: e.name_key.clone(),
        kind: kind_str(e.kind).to_string(),
        parent_iso_code: e
            .parent_id
            .and_then(|pid| lookup.get_entity(pid))
            .map(|p| p.iso_code.clone()),
        total_area_m2: e.total_area_m2,
    }
}

/// All geo entities of the active worldview, optionally filtered to one
/// `kind` ("continent"|"country"|"province"|"city"; `None` = all kinds)
/// and/or to children of `parent_iso_code`. Static per-worldview catalog
/// — journey- and layer-independent; the frontend fetches it once per
/// worldview (refetch after `switch_worldview`).
pub fn get_geo_entities(
    kind: Option<String>,
    parent_iso_code: Option<String>,
) -> Result<Vec<GeoEntityInfo>> {
    let active = crate::api::api::get()
        .achievement
        .active()
        .ok_or_else(|| anyhow::anyhow!("achievement system not initialized"))?;
    let lookup = &active.lookup;

    let kinds: Vec<GeoEntityKind> = match kind {
        Some(k) => vec![parse_kind(&k)?],
        None => vec![
            GeoEntityKind::Continent,
            GeoEntityKind::Country,
            GeoEntityKind::Province,
            GeoEntityKind::City,
        ],
    };

    Ok(kinds
        .into_iter()
        .flat_map(|k| lookup.entities_of_kind(k))
        .filter(|e| match &parent_iso_code {
            None => true,
            Some(pic) => e
                .parent_id
                .and_then(|pid| lookup.get_entity(pid))
                .is_some_and(|p| &p.iso_code == pic),
        })
        .map(|e| entity_info(e, lookup))
        .collect())
}

/// Countries with any explored area in `layer`'s merged bitmap. `layer`
/// is REQUIRED (no implicit Default). Recomputed on every call.
pub fn get_visited_countries(layer: AchievementLayer) -> Result<Vec<GeoEntityInfo>> {
    let state = crate::api::api::get();
    let active = state
        .achievement
        .active()
        .ok_or_else(|| anyhow::anyhow!("achievement system not initialized"))?;
    let lookup = &active.lookup;

    let visited = state.storage.with_journey_snapshot(|snapshot| {
        let bitmap = snapshot.finalized_bitmap(&layer.to_layer_kind(), None)?;
        Ok(crate::achievement::composites::visited_countries(
            &bitmap, None, lookup,
        ))
    })?;

    Ok(visited
        .into_iter()
        .filter_map(|c| match c.region_id {
            RegionId::GeoEntity(id) => lookup.get_entity(id).map(|e| entity_info(e, lookup)),
            RegionId::Poi { .. } => None,
        })
        .collect())
}
