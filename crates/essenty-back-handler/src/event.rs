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
}

impl BackEvent {
    /// Regular back invocation (e.g. hardware back button).
    #[must_use]
    pub fn invoked() -> Self {
        Self { phase: BackPhase::Invoked, progress: None }
    }

    /// Predictive gesture started.
    #[must_use]
    pub fn started() -> Self {
        Self { phase: BackPhase::Started, progress: None }
    }

    /// Predictive gesture progressed. `progress` is conventionally
    /// `0.0..=1.0`; values are stored as given.
    #[must_use]
    pub fn progressed(progress: f32) -> Self {
        Self { phase: BackPhase::Progressed, progress: Some(progress) }
    }

    /// Predictive gesture cancelled.
    #[must_use]
    pub fn cancelled() -> Self {
        Self { phase: BackPhase::Cancelled, progress: None }
    }
}
