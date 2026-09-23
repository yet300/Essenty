use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BackError {
    /// A progress/cancel/invoke arrived with no gesture in flight.
    #[error("no predictive back gesture in progress")]
    NoGestureInProgress,
}
