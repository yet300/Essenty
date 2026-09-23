use essenty_lifecycle::{LifecycleRegistry, LifecycleState};

/// Forwards Android `Activity` lifecycle callbacks into a [`LifecycleRegistry`].
///
/// Each method is idempotent with respect to its target state: calling
/// `on_resume` when already resumed is a no-op returning `Ok(())`, which
/// keeps duplicate platform callbacks harmless. Out-of-order transitions
/// are walked through intermediate states by the core registry.
#[derive(Debug, Clone, Default)]
pub struct AndroidLifecycle {
    registry: LifecycleRegistry,
}

impl AndroidLifecycle {
    /// Creates a host in `Initialized`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrows the underlying registry (for observers and state checks).
    #[must_use]
    pub fn registry(&self) -> &LifecycleRegistry {
        &self.registry
    }

    /// Maps to `Activity.onCreate`.
    ///
    /// # Errors
    ///
    /// Propagates registry transition errors (e.g. already destroyed).
    pub fn on_create(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        if self.registry.state() == LifecycleState::Initialized {
            self.registry.create()
        } else {
            self.registry.move_to(LifecycleState::Created)
        }
    }

    /// Maps to `Activity.onStart`.
    ///
    /// # Errors
    ///
    /// Propagates registry transition errors (e.g. already destroyed).
    pub fn on_start(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        self.registry.move_to(LifecycleState::Started)
    }

    /// Maps to `Activity.onResume`.
    ///
    /// # Errors
    ///
    /// Propagates registry transition errors (e.g. already destroyed).
    pub fn on_resume(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        self.registry.move_to(LifecycleState::Resumed)
    }

    /// Maps to `Activity.onPause`.
    ///
    /// # Errors
    ///
    /// Propagates registry transition errors (e.g. already destroyed).
    pub fn on_pause(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        // Pause from Resumed goes to Started; from Started it is a no-op.
        self.registry.move_to(LifecycleState::Started)
    }

    /// Maps to `Activity.onStop`.
    ///
    /// # Errors
    ///
    /// Propagates registry transition errors (e.g. already destroyed).
    pub fn on_stop(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        self.registry.move_to(LifecycleState::Created)
    }

    /// Maps to `Activity.onDestroy`.
    ///
    /// # Errors
    ///
    /// Returns [`AlreadyDestroyed`](essenty_lifecycle::LifecycleError::AlreadyDestroyed)
    /// when the registry is already destroyed.
    pub fn on_destroy(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        self.registry.destroy()
    }
}
