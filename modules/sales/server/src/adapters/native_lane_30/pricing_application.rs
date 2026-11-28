//! Query port and handler for the native Sales price-list workspace read. No SQL.

use super::pricing_domain::{PriceListQuery, PriceListWorkspace};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type PriceListReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait PriceListRead: Send + Sync {
    fn workspace<'a>(&'a self, query: &'a PriceListQuery) -> PriceListReadFuture<'a, PriceListWorkspace>;
}

pub async fn read_workspace<R: PriceListRead + ?Sized>(
    reader: &R,
    query: &PriceListQuery,
) -> Result<PriceListWorkspace, BusinessError> {
    query.validate()?;
    reader.workspace(query).await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Probe(std::sync::Mutex<usize>);
    impl PriceListRead for Probe {
        fn workspace<'a>(
            &'a self,
            _query: &'a PriceListQuery,
        ) -> PriceListReadFuture<'a, PriceListWorkspace> {
            *self.0.lock().unwrap() += 1;
            Box::pin(async { Ok(PriceListWorkspace::default()) })
        }
    }

    #[tokio::test]
    async fn validates_before_reader() {
        let reader = Probe(std::sync::Mutex::new(0));
        let valid = PriceListQuery { company: "company-1".into() };
        assert!(read_workspace(&reader, &valid).await.is_ok());
        let invalid = PriceListQuery { company: "bad id".into() };
        assert!(read_workspace(&reader, &invalid).await.is_err());
        assert_eq!(*reader.0.lock().unwrap(), 1);
    }
}
