//! Single owner of achievement runtime state: the active worldview
//! context.

use std::sync::{Arc, Mutex, RwLock};

use anyhow::anyhow;

use crate::achievement::geo_lookup::GeoLookupTable;
use crate::storage::Storage;

/// Snapshot bundle describing the currently-active worldview: its id and
/// the geo lookup loaded from that worldview's `geo_data` bin.
#[derive(Clone)]
pub struct ActiveWorldviewCtx {
    pub worldview_id: String,
    pub lookup: Arc<GeoLookupTable>,
}

#[derive(Default)]
pub struct AchievementManager {
    /// `None` until `install_active` has run (app bootstrap).
    active: RwLock<Option<Arc<ActiveWorldviewCtx>>>,
    /// Serializes `install_active` so concurrent installs can't leave the
    /// persisted setting and the published context disagreeing.
    install_lock: Mutex<()>,
}

impl AchievementManager {
    /// Current active context, or `None` before the first `install_active`.
    pub fn active(&self) -> Option<Arc<ActiveWorldviewCtx>> {
        self.active.read().expect("active lock poisoned").clone()
    }

    /// Load `geo_data_bytes`, validate `worldview_id` is in the bin,
    /// persist the choice, then publish the new active context
    /// (last-writer-wins on repeat calls / POV switch). Persist runs
    /// BEFORE the in-memory swap (disk-leads-runtime ordering).
    pub fn install_active(
        &self,
        storage: &Storage,
        worldview_id: &str,
        geo_data_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let _install = self.install_lock.lock().expect("install lock poisoned");

        let lookup = Arc::new(GeoLookupTable::load_from_bytes(geo_data_bytes)?);
        lookup
            .worldviews()
            .iter()
            .find(|w| w.id == worldview_id)
            .ok_or_else(|| anyhow!("worldview `{worldview_id}` not in loaded geo data"))?;

        // Persist FIRST: a failure here leaves the old ctx untouched.
        storage.set_active_worldview(worldview_id)?;

        *self.active.write().expect("active lock poisoned") = Some(Arc::new(ActiveWorldviewCtx {
            worldview_id: worldview_id.to_string(),
            lookup,
        }));
        Ok(())
    }
}
