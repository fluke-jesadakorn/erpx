//! Existing reservation warning wording, shared by native scheduled producers.
#[derive(Clone, Copy)]
pub enum ReservationAlertPhase {
    Expiring,
    Expired,
}
impl ReservationAlertPhase {
    pub fn message(self) -> &'static str {
        match self {
            Self::Expiring => {
                "Stock reservation expires within 24 hours; use, release, or re-reserve it from the source work"
            }
            Self::Expired => {
                "Stock reservation expired and no longer protects availability; re-reserve or correct the source work"
            }
        }
    }
}
