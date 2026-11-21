//! Home read-cursor use case on a caller-owned Holding transaction. The port
//! owns the single upsert; this layer only validates the bounded input.
use crate::domain::home_read_cursor::MarkHomeReadInput;
use erpx_server_shared::public::BusinessError;
use std::future::Future;

pub trait HomeReadCursorRepository: Send {
    type Error: From<BusinessError> + Send;

    fn mark<'a>(
        &'a mut self,
        input: &'a MarkHomeReadInput,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

/// Mark the current workspace as read through `read_through`. The adapter's
/// GREATEST upsert makes replay and out-of-order calls idempotent.
pub async fn mark_home_read<R: HomeReadCursorRepository>(
    repository: &mut R,
    input: &MarkHomeReadInput,
) -> Result<(), R::Error> {
    input.validate().map_err(R::Error::from)?;
    repository.mark(input).await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Reject;
    impl HomeReadCursorRepository for Reject {
        type Error = BusinessError;
        async fn mark(&mut self, _: &MarkHomeReadInput) -> Result<(), Self::Error> {
            panic!("must not be called for invalid input")
        }
    }

    #[tokio::test]
    async fn invalid_input_never_reaches_the_port() {
        let input = MarkHomeReadInput {
            company_id: "c".into(),
            user_id: "".into(),
            workspace_base: "work".into(),
            read_through: 1,
        };
        assert!(mark_home_read(&mut Reject, &input).await.is_err());
    }
}
