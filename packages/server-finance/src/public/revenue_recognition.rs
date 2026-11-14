//! Native Finance revenue-obligation recognition and its owner HTTP ingress.
pub use crate::adapters::revenue_recognition::{
    RevenueRecognitionInput, RevenueRecognitionResult, recognize_revenue,
};
pub use crate::adapters::revenue_recognition_http::{
    RevenueRecognitionHttpBackend, RevenueRecognitionHttpCommandRequest,
    RevenueRecognitionHttpFuture, RevenueRecognitionHttpTarget, RevenueRecognitionResponse,
    revenue_recognition_router,
};
pub use crate::domain::revenue::{RevenueRecognitionOutcome, recognize_revenue_amount};
