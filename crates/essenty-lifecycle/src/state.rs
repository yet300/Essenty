///
/// The ordering (excluding [`LifecycleState::Destroyed`], which is terminal)
/// is `Initialized < Created < Started < Resumed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifecycleState {
    /// Freshly constructed registry; no lifecycle event delivered yet.
    Initialized,
    /// Component created (e.g. `onCreate`).
    Created,
    /// Component started and visible (e.g. `onStart`).
    Started,
    /// Component resumed and interactive (e.g. `onResume`).
    Resumed,
    /// Terminal state. No further transitions are allowed.
    Destroyed,
}

impl LifecycleState {
    /// Returns `true` for [`LifecycleState::Resumed`].
    #[must_use]
    pub fn is_resumed(self) -> bool {
        matches!(self, Self::Resumed)
    }

    /// Returns `true` for [`LifecycleState::Started`] or
    /// [`LifecycleState::Resumed`].
    #[must_use]
    pub fn is_started(self) -> bool {
        matches!(self, Self::Started | Self::Resumed)
    }

    /// Returns `true` for the terminal [`LifecycleState::Destroyed`] state.
    #[must_use]
    pub fn is_destroyed(self) -> bool {
        matches!(self, Self::Destroyed)
    }

    /// Rank along the forward chain, used to walk intermediate states.
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Initialized => 0,
            Self::Created => 1,
            Self::Started => 2,
            Self::Resumed => 3,
            Self::Destroyed => 4,
        }
    }

    /// State for a given non-terminal rank.
    pub(crate) fn from_rank(rank: u8) -> Self {
        match rank {
            0 => Self::Initialized,
            1 => Self::Created,
            2 => Self::Started,
            _ => Self::Resumed,
        }
    }
}
