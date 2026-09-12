//! App-local cancellation for explicit model-list reads. No generation jobs.
use crate::app_state::AppState;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tauri::State;
use webnovel_core::{
    projects::{CoreError, CoreResult},
    providers::adapter::CancellationToken,
};

#[derive(Clone, Default)]
pub struct EndpointDiscovery(Arc<Mutex<HashMap<String, (CancellationToken, bool)>>>);

pub struct DiscoveryRead {
    owner: EndpointDiscovery,
    id: String,
    pub token: CancellationToken,
}

impl EndpointDiscovery {
    fn validate(id: &str) -> CoreResult<()> {
        if id.len() != 36 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            return Err(CoreError::new(
                "InvalidDiscoveryId",
                "Reload API connections and try again.",
            ));
        }
        Ok(())
    }

    pub fn begin(&self, id: String) -> CoreResult<DiscoveryRead> {
        Self::validate(&id)?;
        let mut reads = self
            .0
            .lock()
            .map_err(|_| crate::commands::provider_commands::unavailable())?;
        if reads.len() >= 64 && !reads.contains_key(&id) {
            return Err(CoreError::new(
                "DiscoveryBusy",
                "Finish the current model search before starting another.",
            ));
        }
        let entry = reads.entry(id.clone()).or_default();
        if entry.1 {
            return Err(CoreError::new(
                "DiscoveryBusy",
                "This model search is already running.",
            ));
        }
        entry.1 = true;
        Ok(DiscoveryRead {
            owner: self.clone(),
            id,
            token: entry.0.clone(),
        })
    }

    fn cancel(&self, id: String) -> CoreResult<()> {
        Self::validate(&id)?;
        let mut reads = self
            .0
            .lock()
            .map_err(|_| crate::commands::provider_commands::unavailable())?;
        // Retain an early cancellation until its matching begin, but bound
        // these short-lived UI races. A later search always has a new ID.
        if reads.len() >= 64 && !reads.contains_key(&id) {
            reads.retain(|_, (_, started)| *started);
        }
        if reads.len() >= 64 && !reads.contains_key(&id) {
            return Err(CoreError::new(
                "DiscoveryBusy",
                "Too many model searches are still finishing.",
            ));
        }
        reads.entry(id).or_default().0.cancel();
        Ok(())
    }
}

impl Drop for DiscoveryRead {
    fn drop(&mut self) {
        if let Ok(mut reads) = self.owner.0.lock() {
            reads.remove(&self.id);
        }
    }
}

#[tauri::command]
pub fn cancel_endpoint_discovery(
    discovery_id: String, state: State<'_, AppState>,
) -> CoreResult<()> {
    let app = &*state;
    let state = &app.endpoint_discovery;
    state.cancel(discovery_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "00000000-0000-0000-0000-000000000001";
    #[test]
    fn cancellation_is_owned_and_survives_arriving_before_begin() {
        let state = EndpointDiscovery::default();
        state.cancel(ID.to_owned()).unwrap();
        let read = state.begin(ID.to_owned()).unwrap();
        assert!(read.token.is_cancelled());
        assert!(state.begin(ID.to_owned()).is_err());
        drop(read);
        assert!(state.0.lock().unwrap().is_empty());
        let next = state.begin(ID.to_owned()).unwrap();
        assert!(!next.token.is_cancelled());
        state.cancel(ID.to_owned()).unwrap();
        assert!(next.token.is_cancelled());
    }
}
