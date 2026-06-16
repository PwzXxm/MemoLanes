use std::collections::HashMap;

use crate::achievement::geo_lookup_storage::BorderStore;
use crate::journey_bitmap::{MAP_WIDTH, TILE_WIDTH};

use super::geo_entity::{GeoEntity, GeoEntityId, GeoEntityKind, Worldview};

pub use geo_data_format::TileMembership;

/// In-memory tile classification. Mirrors the on-disk
/// `geo_data_format::TileMembership` except `Border` carries a `u32`
/// index into the local `BorderStore`. Constructed only by
/// `GeoLookupTable::load_from_bytes` (and the test-only constructors).
#[derive(Debug, Clone)]
pub(crate) enum MemTileMembership {
    Single(GeoEntityId),
    Border(u32),
    None,
}

/// Hierarchical geo lookup table operating at tile and block granularity.
/// Reuses the JourneyBitmap coordinate space.
pub struct GeoLookupTable {
    /// Tile level: flat array indexed by tile_y * MAP_WIDTH + tile_x.
    /// Border variants carry an id into `border_store`.
    tile_lookup: Vec<MemTileMembership>,

    /// Border-tile store (strategy chosen by the `BorderStore` alias).
    border_store: BorderStore,

    /// All entities keyed by ID.
    entities: HashMap<GeoEntityId, GeoEntity>,

    worldviews: Vec<Worldview>,

    /// The bin's provenance hash, mirrored from the on-disk header.
    provenance_hash: [u8; 32],
}

impl GeoLookupTable {
    pub fn load_from_bytes(data: &[u8]) -> anyhow::Result<Self> {
        use geo_data_format::TileEntry;

        let gd = geo_data_format::read_geo_data(data)?;

        let tile_lookup: Vec<MemTileMembership> = gd
            .tile_index
            .into_iter()
            .map(|e| match e {
                TileEntry::Single(v) => MemTileMembership::Single(v),
                TileEntry::None => MemTileMembership::None,
                TileEntry::Border(id) => MemTileMembership::Border(id),
            })
            .collect();

        let border_store = BorderStore::from_compressed(gd.border_blobs);

        let entities: HashMap<GeoEntityId, GeoEntity> =
            gd.entities.into_iter().map(|e| (e.id, e)).collect();

        Ok(Self {
            tile_lookup,
            border_store,
            entities,
            worldviews: gd.worldviews,
            provenance_hash: gd.provenance_hash,
        })
    }

    pub fn lookup(
        &self,
        tile_x: u16,
        tile_y: u16,
        block_x: u8,
        block_y: u8,
    ) -> Option<GeoEntityId> {
        debug_assert!(
            (block_x as i64) < TILE_WIDTH && (block_y as i64) < TILE_WIDTH,
            "block coord out of range: ({block_x}, {block_y})"
        );
        let tile_idx = tile_y as usize * MAP_WIDTH as usize + tile_x as usize;
        match &self.tile_lookup[tile_idx] {
            MemTileMembership::Single(v) => Some(*v),
            MemTileMembership::None => None,
            MemTileMembership::Border(id) => {
                let cell_idx = block_y as usize * TILE_WIDTH as usize + block_x as usize;
                self.border_store.lookup(*id, cell_idx)
            }
        }
    }

    pub fn get_entity(&self, id: GeoEntityId) -> Option<&GeoEntity> {
        self.entities.get(&id)
    }

    pub fn entity_kind(&self, id: GeoEntityId) -> Option<GeoEntityKind> {
        self.entities.get(&id).map(|e| e.kind)
    }

    pub fn ancestor_of_kind(
        &self,
        entity_id: GeoEntityId,
        kind: GeoEntityKind,
    ) -> Option<GeoEntityId> {
        let entity = self.entities.get(&entity_id)?;
        if entity.kind == kind {
            return Some(entity_id);
        }
        match entity.parent_id {
            Some(parent_id) => self.ancestor_of_kind(parent_id, kind),
            None => None,
        }
    }

    pub fn entities_of_kind(&self, kind: GeoEntityKind) -> Vec<&GeoEntity> {
        self.entities.values().filter(|e| e.kind == kind).collect()
    }

    pub fn worldviews(&self) -> &[Worldview] {
        &self.worldviews
    }

    pub fn provenance_hash(&self) -> [u8; 32] {
        self.provenance_hash
    }

    /// Resident heap bytes held by the border-tile store. Meaning depends
    /// on the active strategy (retained compressed blobs for lazy, dense
    /// arrays for plain). For diagnostics.
    pub fn border_resident_heap_bytes(&self) -> usize {
        self.border_store.resident_heap_bytes()
    }

    #[doc(hidden)]
    pub fn empty_for_test() -> Self {
        Self {
            tile_lookup: vec![MemTileMembership::None; (MAP_WIDTH * MAP_WIDTH) as usize],
            border_store: BorderStore::build(Vec::new()),
            entities: HashMap::new(),
            worldviews: vec![],
            provenance_hash: [0u8; 32],
        }
    }
}
