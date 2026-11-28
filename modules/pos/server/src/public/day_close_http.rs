//! Native POS day-close command surface. Composition implements the backend
//! trait and mounts [`pos_day_close_router`] on the private service; the owner
//! crate supplies the pure decision rules, use case and owner SQL only.
pub use crate::adapters::day_close_http::{
    DayCloseHttpBackend, DayCloseHttpFuture, DayCloseHttpRequest, DayCloseHttpResponse,
    DayCloseHttpTarget, pos_day_close_router,
};
pub use crate::application::day_close::{
    DayCloseCommand, DayCloseReport, DayCloseWitness, execute_day_close,
};
