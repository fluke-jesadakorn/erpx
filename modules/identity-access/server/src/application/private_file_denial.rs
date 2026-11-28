//! Native private-file denial authority witness use case.
//!
//! The owner transaction and the closed fact image stay behind
//! [`PrivateFileDenialRepository`]. This layer only binds validated authority
//! facts to the image and returns the opaque equality token; it never opens a
//! transaction, reads an HTTP request or exposes the underlying rows.
use crate::domain::private_file_denial::{
    PrivateFileDenialActor, witness_hash, witness_image_required,
};
use erpx_platform_shared_server::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type PrivateFileDenialFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PrivateFileDenialRepository {
    type Error: From<BusinessError>;

    /// A closed, deterministic JSON text of the authority facts. The adapter
    /// reads only the owner's Holding transaction; no caller SQL is accepted.
    fn image<'a>(
        &'a self,
        actor: &'a PrivateFileDenialActor,
    ) -> PrivateFileDenialFuture<'a, String, Self::Error>;
}

pub async fn private_file_denial_witness<R: PrivateFileDenialRepository>(
    repository: &mut R,
    actor: &PrivateFileDenialActor,
) -> Result<String, R::Error> {
    let image = repository.image(actor).await?;
    if image.is_empty() {
        return Err(witness_image_required().into());
    }
    Ok(witness_hash(actor, &image))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Memory {
        image: String,
    }

    impl PrivateFileDenialRepository for Memory {
        type Error = BusinessError;
        fn image<'a>(
            &'a self,
            _: &'a PrivateFileDenialActor,
        ) -> PrivateFileDenialFuture<'a, String, Self::Error> {
            Box::pin(async move { Ok(self.image.clone()) })
        }
    }

    #[tokio::test]
    async fn witness_binds_the_actor_to_the_owner_image() {
        let actor = PrivateFileDenialActor::parse(&json!({
            "companyId": "company-1", "userId": "user-1",
            "assignmentId": "assignment-1", "role": "it_admin", "sessionId": null
        }))
        .unwrap();
        let mut repository = Memory {
            image: r#"{"chain":[],"people":{}}"#.into(),
        };
        let first = private_file_denial_witness(&mut repository, &actor)
            .await
            .unwrap();
        assert_eq!(first.len(), 64);
        assert_eq!(
            first,
            private_file_denial_witness(&mut repository, &actor)
                .await
                .unwrap()
        );
        repository.image = String::new();
        assert_eq!(
            private_file_denial_witness(&mut repository, &actor)
                .await
                .unwrap_err()
                .code,
            "PRIVATE_FILE_DENIAL_IMAGE_REQUIRED"
        );
    }
}
