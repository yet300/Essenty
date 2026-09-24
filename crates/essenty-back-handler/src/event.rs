/// Phase of a back event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackPhase {
    /// Predictive gesture started.
    Started,
    /// Predictive gesture progressed (carries progress in the event).
    Progressed,
    /// Predictive gesture cancelled.
    Cancelled,
    /// Back invoked (regular press or predictive completion).
    Invoked,
}

/// Edge from which a predictive gesture begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwipeEdge {
    /// Gesture began at the left edge.
    Left,
    /// Gesture began at the right edge.
    Right,
    /// The platform did not identify a known edge.
    Unknown,
}

/// Platform-neutral position accompanying predictive start or progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GesturePosition {
    /// Edge reported by the platform.
    pub swipe_edge: SwipeEdge,
    /// Horizontal touch position reported by the platform.
    pub touch_x: f32,
    /// Vertical touch position reported by the platform.
    pub touch_y: f32,
}

/// Back event delivered to callbacks.
///
/// Progress (`0.0..=1.0` by convention) is present only for
/// [`BackPhase::Progressed`]; all other phases carry `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackEvent {
    /// Which phase this event represents.
    pub phase: BackPhase,
    /// Gesture progress; `Some` only for `Progressed`.
    pub progress: Option<f32>,
    /// Gesture position, when provided for start or progress.
    pub position: Option<GesturePosition>,
}

impl BackEvent {
    /// Regular back invocation (e.g. hardware back button).
    #[must_use]
    pub fn invoked() -> Self {
        Self { phase: BackPhase::Invoked, progress: None, position: None }
    }

    /// Predictive gesture started.
    #[must_use]
    pub fn started() -> Self {
        Self { phase: BackPhase::Started, progress: None, position: None }
    }

    /// Predictive gesture started with platform position data.
    #[must_use]
    pub fn started_with(position: GesturePosition) -> Self {
        Self { phase: BackPhase::Started, progress: None, position: Some(position) }
    }

    /// Predictive gesture progressed. `progress` is conventionally
    /// `0.0..=1.0`; values are stored as given.
    #[must_use]
    pub fn progressed(progress: f32) -> Self {
        Self { phase: BackPhase::Progressed, progress: Some(progress), position: None }
    }

    /// Predictive gesture progressed with platform position data.
    #[must_use]
    pub fn progressed_with(progress: f32, position: GesturePosition) -> Self {
        Self { phase: BackPhase::Progressed, progress: Some(progress), position: Some(position) }
    }

    /// Predictive gesture cancelled.
    #[must_use]
    pub fn cancelled() -> Self {
        Self { phase: BackPhase::Cancelled, progress: None, position: None }
    }
}
