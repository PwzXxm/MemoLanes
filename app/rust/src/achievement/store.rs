//! The achievement-store seam: a trait pair the storage/api layers consume
//! without binding to a concrete store. The compute-on-demand
//! [`OnDemandStore`](super::on_demand::OnDemandStore) implements it. Mirrors
//! `cache_db`'s `CacheDb` trait + `new()` factory.

use anyhow::Result;
use geo_data_format::Pov;

use crate::achievement::region_index::RegionStateMap;
use crate::achievement::scope::AchievementLayer;
use crate::geo::GeoLookup;
use crate::journey_snapshot::JourneySnapshot;

/// The long-lived achievement engine owned by `Storage.dbs`. Holds the active
/// POV geo.
pub trait AchievementStore: Send {
    /// A committed journey change. This store recomputes per read, so it is a
    /// no-op; an implementation with cached state would invalidate it here.
    fn invalidate_all(&self) -> Result<()>;

    /// Supply (or switch) the active POV's geo lookup, kept for region reads.
    /// A POV change re-derives POV-scoped state.
    fn set_geo(&mut self, pov: Pov, geo: Box<dyn GeoLookup + Send>) -> Result<()>;

    /// Open a consistent reader over `snapshot`: binds the snapshot and computes
    /// lazily in the read methods. The reader borrows both `self` (geo)
    /// and `snapshot`, so every value it serves reflects one snapshot.
    fn reader<'a>(
        &'a self,
        snapshot: &'a JourneySnapshot,
    ) -> Result<Box<dyn AchievementReader + 'a>>;
}

/// The read surface the api layer consumes, under one snapshot.
pub trait AchievementReader {
    /// Explored area (m²) for one layer (0 if absent).
    fn explored_area_m2(&self, layer: AchievementLayer) -> Result<u64>;

    /// All region states for the active POV.
    fn region_states(&self) -> Result<RegionStateMap>;

    /// The active POV's geo lookup, or `None` until `set_geo`.
    fn geo(&self) -> Option<&dyn GeoLookup>;

    /// The active POV, or `None` until `set_geo`.
    fn active_pov(&self) -> Option<Pov>;
}
