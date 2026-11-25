//! Owner query port for the lane 33 release child-image read. No SQL, no axum.

use crate::adapters::native_lane_33::domain::ReleaseChildImage;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{future::Future, pin::Pin, sync::Arc};

pub type ReleaseChildImageFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The owner executes the acceptance-row and child-aggregate reads on its own
/// pool. The router only transports a company and a release identity.
pub trait ReleaseChildImageRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: &'a str,
        release: &'a str,
    ) -> ReleaseChildImageFuture<'a, Option<ReleaseChildImage>>;
}

pub type SharedReleaseChildImageRead = Arc<dyn ReleaseChildImageRead>;

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLATFORM_RELEASE_CHILD_IMAGE_UNAVAILABLE",
        "The native Platform release child-image service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Down;
    impl ReleaseChildImageRead for Down {
        fn read<'a>(
            &'a self,
            _company: &'a str,
            _release: &'a str,
        ) -> ReleaseChildImageFuture<'a, Option<ReleaseChildImage>> {
            Box::pin(async { Err(unavailable()) })
        }
    }

    #[tokio::test]
    async fn reader_fails_closed() {
        let error = Down.read("company", "release").await.unwrap_err();
        assert_eq!(error.code, "PLATFORM_RELEASE_CHILD_IMAGE_UNAVAILABLE");
        assert_eq!(error.kind, FailureKind::Unavailable);
    }
}
