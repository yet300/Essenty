use essenty_state_keeper::StateKeeper;
use std::collections::BTreeMap;

/// Hosts saved-state bytes across Android process recreation.
///
/// Thin wrapper around [`StateKeeper`] documenting the intended
/// `SavedStateRegistry` wiring: `consume` restored bytes once after
/// recreation, `register` live providers, then `perform_save` when the
/// platform requests a snapshot. Byte format stays caller-defined.
#[derive(Debug, Default)]
pub struct AndroidStateHost {
    keeper: StateKeeper,
}

impl AndroidStateHost {
    /// Empty host with no restored state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Host preloaded with bytes from `SavedStateRegistry.consumeRestoredStateForKey`
    /// equivalents.
    #[must_use]
    pub fn with_restored(restored: BTreeMap<String, Vec<u8>>) -> Self {
        Self { keeper: StateKeeper::with_restored(restored) }
    }

    /// Borrows the underlying keeper mutably for provider registration and
    /// single-shot consumption.
    #[must_use]
    pub fn keeper_mut(&mut self) -> &mut StateKeeper {
        &mut self.keeper
    }

    /// Borrows the underlying keeper.
    #[must_use]
    pub fn keeper(&self) -> &StateKeeper {
        &self.keeper
    }

    /// Produces the snapshot to hand to the platform
    /// (`onSaveInstanceState` equivalent).
    ///
    /// # Errors
    ///
    /// Propagates [`StateKeeperError::Encode`](essenty_state_keeper::StateKeeperError::Encode)
    /// from failing providers.
    pub fn perform_save(
        &self,
    ) -> Result<BTreeMap<String, Vec<u8>>, essenty_state_keeper::StateKeeperError> {
        self.keeper.save()
    }
}
