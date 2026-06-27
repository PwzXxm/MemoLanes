//! Achievement statistics: explored-area and (in later slices)
//! geo-entity coverage computed from journey data.
pub mod explored_area;
pub mod on_demand;
pub mod region_index;
pub mod region_read;
pub mod scope;
pub mod store;

use anyhow::Result;

use store::AchievementStore;

/// Construct the achievement store: a compute-on-demand
/// [`on_demand::OnDemandStore`], mirroring `cache_db::new`.
pub fn new(_cache_dir: &str) -> Result<Box<dyn AchievementStore + Send>> {
    Ok(Box::new(on_demand::OnDemandStore::new()))
}
