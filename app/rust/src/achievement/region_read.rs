//! Layered region read-model: loaded `RegionIndex` state joined with the
//! POV geo tree into list / progress / detail shapes. Pure functions over
//! `(states, geo)`; the `api/achievement` FRB functions load the state and wrap.

use std::collections::HashMap;

use chrono::NaiveDate;
use geo_data_format::{GeoEntityId, GeoEntityKind};

use crate::achievement::region_index::RegionStateMap;
use crate::achievement::scope::AchievementLayer;
use crate::geo::GeoLookup;

/// Geo admin level (mirrors `GeoEntityKind` for the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Continent,
    Country,
    Province,
    City,
}

impl From<GeoEntityKind> for RegionKind {
    fn from(k: GeoEntityKind) -> Self {
        match k {
            GeoEntityKind::Continent => RegionKind::Continent,
            GeoEntityKind::Country => RegionKind::Country,
            GeoEntityKind::Province => RegionKind::Province,
            GeoEntityKind::City => RegionKind::City,
        }
    }
}

impl RegionKind {
    fn to_geo(self) -> GeoEntityKind {
        match self {
            RegionKind::Continent => GeoEntityKind::Continent,
            RegionKind::Country => GeoEntityKind::Country,
            RegionKind::Province => GeoEntityKind::Province,
            RegionKind::City => GeoEntityKind::City,
        }
    }
}

pub struct RegionLevel {
    pub kind: RegionKind,
    pub total: u32,
}

/// One entity at a level, in the queried layer. Unvisited → zeros / `None`.
pub struct RegionEntry {
    pub entity_id: u32,
    pub kind: RegionKind,
    pub name_key: String,
    pub visited_area_m2: u64,
    pub total_area_m2: u64,
    pub first_visit_date: Option<NaiveDate>,
    pub completed_at: Option<NaiveDate>,
}

/// Everything a level screen needs for `(layer, level, parent)` — one snapshot:
/// counts, completion badge, and the full entity list (visited + unvisited).
pub struct RegionLevelView {
    pub level: RegionKind,
    pub visited: u32,
    pub total: u32,
    /// Set iff every entity visited (`visited == total > 0`): the unlock date
    /// (latest first-visit). The completion badge — `None` = not yet complete.
    pub completed_at: Option<NaiveDate>,
    /// All entities in scope, visited and unvisited — drives list + grey-out.
    pub entries: Vec<RegionEntry>,
}

/// One layer's slice of an entity's coverage (unvisited → zero / `None`).
pub struct RegionLayerStat {
    pub visited_area_m2: u64,
    pub percentage: f32,
    pub first_visit_date: Option<NaiveDate>,
    pub completed_at: Option<NaiveDate>,
}

pub struct RegionNode {
    pub entity_id: u32,
    pub kind: RegionKind,
    pub name_key: String,
    pub total_area_m2: u64,
    pub by_layer: HashMap<AchievementLayer, RegionLayerStat>,
}

/// One entity plus its direct children (flat drill-down: fetch a child's own
/// detail with `region_detail(child.entity_id)`).
pub struct RegionDetail {
    pub node: RegionNode,
    pub children: Vec<RegionNode>,
}

fn area_ratio(area: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (area as f64 / total as f64) as f32
    }
}

fn entities_in_scope(
    geo: &dyn GeoLookup,
    level: GeoEntityKind,
    parent: Option<GeoEntityId>,
) -> Vec<GeoEntityId> {
    geo.entities_of_kind(level)
        .iter()
        .copied()
        .filter(|&id| geo.entity(id).map(|e| e.parent_id) == Some(parent))
        .collect()
}

fn region_node(
    states: &RegionStateMap,
    geo: &dyn GeoLookup,
    id: GeoEntityId,
) -> Option<RegionNode> {
    let entity = geo.entity(id)?;
    let by_layer = AchievementLayer::ALL_LAYERS
        .into_iter()
        .map(|layer| {
            let stat = states.get(&(layer, id)).map_or(
                RegionLayerStat {
                    visited_area_m2: 0,
                    percentage: 0.0,
                    first_visit_date: None,
                    completed_at: None,
                },
                |s| RegionLayerStat {
                    visited_area_m2: s.visited_area_m2,
                    percentage: area_ratio(s.visited_area_m2, entity.total_area_m2),
                    first_visit_date: Some(s.first_visit_date),
                    completed_at: s.completed_at,
                },
            );
            (layer, stat)
        })
        .collect();
    Some(RegionNode {
        entity_id: id.0,
        kind: entity.kind.into(),
        name_key: entity.name_key.clone(),
        total_area_m2: entity.total_area_m2,
        by_layer,
    })
}

/// Levels present in the current map (layer-independent).
pub fn region_levels(geo: &dyn GeoLookup) -> Vec<RegionLevel> {
    [
        GeoEntityKind::Continent,
        GeoEntityKind::Country,
        GeoEntityKind::Province,
        GeoEntityKind::City,
    ]
    .into_iter()
    .filter_map(|kind| {
        let total = geo.entities_of_kind(kind).len() as u32;
        (total > 0).then_some(RegionLevel {
            kind: kind.into(),
            total,
        })
    })
    .collect()
}

/// Joined view for `level` within `parent` in one layer: counts, completion
/// badge, and the full entity list (visited + unvisited). The badge unlocks on
/// the last first-visit, set only when every entity in scope is visited.
pub fn region_level_view(
    states: &RegionStateMap,
    geo: &dyn GeoLookup,
    layer: AchievementLayer,
    level: RegionKind,
    parent: Option<u32>,
) -> RegionLevelView {
    let mut entries = Vec::new();
    let mut visited = 0u32;
    let mut completed_at = None;
    let mut all_visited = true;

    for id in entities_in_scope(geo, level.to_geo(), parent.map(GeoEntityId)) {
        let Some(entity) = geo.entity(id) else {
            continue;
        };
        let (visited_area_m2, first_visit_date, entity_completed_at) =
            match states.get(&(layer, id)) {
                Some(s) => {
                    visited += 1;
                    completed_at = completed_at.max(Some(s.first_visit_date));
                    (s.visited_area_m2, Some(s.first_visit_date), s.completed_at)
                }
                None => {
                    all_visited = false;
                    (0, None, None)
                }
            };
        entries.push(RegionEntry {
            entity_id: id.0,
            kind: entity.kind.into(),
            name_key: entity.name_key.clone(),
            visited_area_m2,
            total_area_m2: entity.total_area_m2,
            first_visit_date,
            completed_at: entity_completed_at,
        });
    }

    let total = entries.len() as u32;
    RegionLevelView {
        level,
        visited,
        total,
        completed_at: (all_visited && total > 0).then_some(completed_at).flatten(),
        entries,
    }
}

/// One entity's per-layer breakdown plus its direct children.
pub fn region_detail(
    states: &RegionStateMap,
    geo: &dyn GeoLookup,
    entity_id: u32,
) -> Option<RegionDetail> {
    let id = GeoEntityId(entity_id);
    let node = region_node(states, geo, id)?;
    let children = geo
        .children(id)
        .iter()
        .filter_map(|&child| region_node(states, geo, child))
        .collect();
    Some(RegionDetail { node, children })
}
