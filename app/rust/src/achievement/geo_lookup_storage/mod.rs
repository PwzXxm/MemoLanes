//! Border-tile store abstraction. Call sites use the concrete
//! `BorderStore` alias below so the hot `lookup` path stays monomorphic.

use geo_data_format::GeoEntityId;

mod plain;

/// Shared surface every border-tile store provides.
#[allow(dead_code)] // Not every method is exercised yet.
pub(crate) trait BorderTileLookup {
    fn from_compressed(blobs: Vec<Box<[u8]>>) -> Self
    where
        Self: Sized;
    fn build(tiles: Vec<Vec<Option<GeoEntityId>>>) -> Self
    where
        Self: Sized;
    fn lookup(&self, id: u32, cell_idx: usize) -> Option<GeoEntityId>;
    fn resident_heap_bytes(&self) -> usize;
}

/// The active border-tile store strategy.
pub(crate) type BorderStore = plain::PlainBorderStore;
