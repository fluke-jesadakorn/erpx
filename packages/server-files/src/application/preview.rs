use crate::domain::preview::{PreviewComputeBudget, PreviewFailure, PreviewInput, PreviewResult};
use std::future::Future;

/// Evidence-only computation. Callers must separately retain current owner,
/// source, scanner and exact lease proofs before making output available.
pub trait PreviewRenderer: Send + Sync {
    fn render(
        &self,
        input: PreviewInput,
        budget: PreviewComputeBudget,
    ) -> impl Future<Output = Result<PreviewResult, PreviewFailure>> + Send;
}
