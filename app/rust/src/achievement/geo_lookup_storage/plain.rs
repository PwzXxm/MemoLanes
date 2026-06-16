//! Plain border-tile store: eagerly expands every blob into a dense
//! array at load. No retained compression, no palette, no LRU.

use geo_data_format::{GeoEntityId, PackedTile, CELLS_PER_TILE};

#[doc(hidden)]
#[allow(dead_code)] // Not every method is exercised yet.
pub struct PlainBorderStore {
    tiles: Vec<Box<[Option<GeoEntityId>]>>,
}

#[allow(dead_code)] // Not every method is exercised yet.
impl PlainBorderStore {
    // `from_compressed` is the public construction path; `build` is an
    // internal helper for tests and `new_synthetic`, hence `pub(crate)`.
    pub(crate) fn build(tiles: Vec<Vec<Option<GeoEntityId>>>) -> Self {
        Self {
            tiles: tiles
                .into_iter()
                .map(|cells| {
                    debug_assert_eq!(
                        cells.len(),
                        CELLS_PER_TILE,
                        "PlainBorderStore::build: tile must have CELLS_PER_TILE cells"
                    );
                    cells.into_boxed_slice()
                })
                .collect(),
        }
    }

    #[doc(hidden)]
    pub fn from_compressed(blobs: Vec<Box<[u8]>>) -> Self {
        let tiles = blobs
            .into_iter()
            .map(|blob| {
                PackedTile::from_compressed_bytes(&blob)
                    .to_dense()
                    .into_boxed_slice()
            })
            .collect();
        Self { tiles }
    }

    pub(crate) fn lookup(&self, id: u32, cell_idx: usize) -> Option<GeoEntityId> {
        self.tiles[id as usize][cell_idx]
    }

    #[doc(hidden)]
    pub fn resident_heap_bytes(&self) -> usize {
        self.tiles.len() * CELLS_PER_TILE * std::mem::size_of::<Option<GeoEntityId>>()
    }
}

impl super::BorderTileLookup for PlainBorderStore {
    fn from_compressed(blobs: Vec<Box<[u8]>>) -> Self {
        Self::from_compressed(blobs)
    }
    fn build(tiles: Vec<Vec<Option<GeoEntityId>>>) -> Self {
        Self::build(tiles)
    }
    fn lookup(&self, id: u32, cell_idx: usize) -> Option<GeoEntityId> {
        Self::lookup(self, id, cell_idx)
    }
    fn resident_heap_bytes(&self) -> usize {
        Self::resident_heap_bytes(self)
    }
}
