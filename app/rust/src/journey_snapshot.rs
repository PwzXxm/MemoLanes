/* Journeys are stored one-by-one, but most consumers (map rendering,
   achievement stats, the time machine) need them merged into a single
   `JourneyBitmap`. `JourneySnapshot` is a read-only, transaction-bound
   view that bundles the `(txn, cache_db)` pair so several reads compose
   into ONE consistent snapshot. It exposes only reads — the cache's
   mutating ops, and the main_db<->cache_db sync invariant, stay private
   to `Storage` (which hands out snapshots via `with_journey_snapshot`).
*/
use crate::{
    cache_db::{CacheDb, LayerKind},
    journey_bitmap::JourneyBitmap,
    journey_header::JourneyKind,
    journey_vector::JourneyVector,
    main_db,
};
use anyhow::Result;
use chrono::NaiveDate;

pub struct JourneySnapshot<'a, 'txn> {
    txn: &'a main_db::Txn<'txn>,
    cache_db: &'a dyn CacheDb,
}

impl<'a, 'txn> JourneySnapshot<'a, 'txn> {
    pub(crate) fn new(txn: &'a main_db::Txn<'txn>, cache_db: &'a dyn CacheDb) -> Self {
        Self { txn, cache_db }
    }

    /// Finalized explored coverage for one layer. `range = None` →
    /// all-time (served from cache); `Some((from, to))` → that inclusive
    /// window (computed directly from main_db, not cached).
    pub fn finalized_bitmap(
        &self,
        layer: &LayerKind,
        range: Option<(NaiveDate, NaiveDate)>,
    ) -> Result<JourneyBitmap> {
        self.cache_db.get_or_compute(self.txn, layer, range)
    }

    /// The not-yet-finalized ongoing journey, if any. Read through the
    /// same snapshot as `finalized_bitmap`, so a caller merging the two
    /// (e.g. the live map renderer) sees one consistent state.
    pub fn ongoing_journey(&self) -> Result<Option<JourneyVector>> {
        self.txn.get_ongoing_journey(None)
    }

    /// Every finalized journey's `(date, kind, footprint)` in ascending
    /// `journey_date` order, for the region / per-journey views to replay.
    /// `query_journeys` is newest-first, hence the reverse.
    pub fn journeys_chronological(&self) -> Result<Vec<(NaiveDate, JourneyKind, JourneyBitmap)>> {
        let mut headers = self.txn.query_journeys(None, None)?;
        headers.reverse();
        let mut out = Vec::with_capacity(headers.len());
        for header in headers {
            let data = self.txn.get_journey_data(&header.id)?;
            let mut bitmap = JourneyBitmap::new();
            data.merge_into(&mut bitmap);
            out.push((header.journey_date, header.journey_kind, bitmap));
        }
        Ok(out)
    }
}
