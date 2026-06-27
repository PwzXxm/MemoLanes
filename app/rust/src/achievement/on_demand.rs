//! Compute-on-demand achievement store: no cache, no persistence. Holds only
//! the active POV geo and computes every read from the journey snapshot via the
//! pure functions in this module.

use anyhow::Result;
use geo_data_format::Pov;

use crate::achievement::explored_area::explored_areas_from_snapshot;
use crate::achievement::region_index::{compute_region_states, JourneyFootprint, RegionStateMap};
use crate::achievement::scope::AchievementLayer;
use crate::achievement::store::{AchievementReader, AchievementStore};
use crate::geo::GeoLookup;
use crate::journey_snapshot::JourneySnapshot;

#[derive(Default)]
pub struct OnDemandStore {
    geo: Option<Box<dyn GeoLookup + Send>>,
    pov: Option<Pov>,
}

impl OnDemandStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AchievementStore for OnDemandStore {
    /// No persisted state to invalidate — every read recomputes from source.
    fn invalidate_all(&self) -> Result<()> {
        Ok(())
    }

    fn set_geo(&mut self, pov: Pov, geo: Box<dyn GeoLookup + Send>) -> Result<()> {
        self.pov = Some(pov);
        self.geo = Some(geo);
        Ok(())
    }

    fn reader<'a>(
        &'a self,
        snapshot: &'a JourneySnapshot,
    ) -> Result<Box<dyn AchievementReader + 'a>> {
        Ok(Box::new(OnDemandReader {
            snapshot,
            geo: self.geo.as_deref().map(|g| g as &dyn GeoLookup),
            pov: self.pov,
        }))
    }
}

/// One reader bound to a snapshot; computes on demand from the shared pure fns.
struct OnDemandReader<'a, 'snap, 'txn> {
    snapshot: &'a JourneySnapshot<'snap, 'txn>,
    geo: Option<&'a dyn GeoLookup>,
    pov: Option<Pov>,
}

impl AchievementReader for OnDemandReader<'_, '_, '_> {
    fn explored_area_m2(&self, layer: AchievementLayer) -> Result<u64> {
        Ok(explored_areas_from_snapshot(self.snapshot, &[layer])?
            .remove(&layer)
            .unwrap_or(0))
    }

    fn region_states(&self) -> Result<RegionStateMap> {
        // No geo → no regions (empty until `set_geo`).
        let Some(geo) = self.geo else {
            return Ok(RegionStateMap::new());
        };
        let journeys = self
            .snapshot
            .journeys_chronological()?
            .into_iter()
            .map(|(date, kind, bitmap)| JourneyFootprint { date, kind, bitmap });
        Ok(compute_region_states(journeys, geo))
    }

    fn geo(&self) -> Option<&dyn GeoLookup> {
        self.geo
    }

    fn active_pov(&self) -> Option<Pov> {
        self.pov
    }
}
