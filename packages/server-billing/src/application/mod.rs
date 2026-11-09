pub mod cost;
pub mod workspace;

pub type BillingFuture<'a, T, E> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send + 'a>>;
pub mod forecast_read;
pub mod target;
pub mod target_assignment;
pub mod target_rollout;
