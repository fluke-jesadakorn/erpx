//! Query port and owner handler for the native Legal executed-copy read.
//!
//! This module contains no SQL. It names the bounded facts the Postgres adapter
//! must supply and composes them into the authoritative response. The caller's
//! Company and user identities travel as opaque bounded values; the adapter
//! joins `legal_items` so every fact is Company-scoped at execution time.

use std::{future::Future, pin::Pin};

use erpx_platform_shared_server::public::BusinessError;

use super::domain::{
    CitationChunk, CopyAuthors, CopyHead, ExecutedCopyRead, ReviewRow, TrustMaterial,
    TrustMaterialDetail,
};

pub type LaneFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The five owner-local facts behind one executed-copy detail read. Composition
/// injects the Pg implementation; there is no permissive or default reader.
pub trait ExecutedCopySource: Send + Sync {
    fn list<'a>(
        &'a self,
        company: &'a str,
        item: &'a str,
    ) -> LaneFuture<'a, Vec<CopyHead>>;
    fn authors<'a>(
        &'a self,
        company: &'a str,
        item: &'a str,
        copy: &'a str,
    ) -> LaneFuture<'a, Option<CopyAuthors>>;
    fn reviews<'a>(
        &'a self,
        company: &'a str,
        copy: &'a str,
    ) -> LaneFuture<'a, Vec<(String, ReviewRow)>>;
    fn member<'a>(
        &'a self,
        company: &'a str,
        item: &'a str,
        user: &'a str,
    ) -> LaneFuture<'a, bool>;
    fn approval<'a>(
        &'a self,
        company: &'a str,
        item: &'a str,
    ) -> LaneFuture<'a, Option<i64>>;
}

/// Company-scoped trust-material facts. The PDF verifier needs the approved
/// set; the trust command needs one bounded row witness. Both are owner-owned
/// reads with no TypeScript SQL behind them.
pub trait TrustMaterialSource: Send + Sync {
    fn approved<'a>(&'a self, company: &'a str) -> LaneFuture<'a, Vec<TrustMaterial>>;
    fn material<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> LaneFuture<'a, Option<TrustMaterialDetail>>;
}

/// Owner-local citation evidence facts. The Files availability check stays
/// outside this port: no Files port is available to this lane, so that read is
/// deliberately retained in TypeScript rather than duplicated as owner SQL.
pub trait CitationEvidenceSource: Send + Sync {
    fn chunk<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> LaneFuture<'a, Option<CitationChunk>>;
    fn text_approved<'a>(
        &'a self,
        company: &'a str,
        review: &'a str,
        document: &'a str,
    ) -> LaneFuture<'a, bool>;
}

/// Owner handler. The query order is fixed: copies first, then their bounded
/// per-copy facts. Every fact is read from the same injected source, so a
/// reconfigured source cannot mix Company scopes.
pub async fn read_executed_copies(
    source: &dyn ExecutedCopySource,
    company: &str,
    user: &str,
    item: &str,
) -> Result<ExecutedCopyRead, BusinessError> {
    let heads = source.list(company, item).await?;
    let mut authors = Vec::with_capacity(heads.len());
    let mut reviews = Vec::new();
    for head in &heads {
        if let Some(author) = source.authors(company, item, &head.id).await? {
            authors.push(author);
            reviews.extend(source.reviews(company, &head.id).await?);
        }
    }
    let member = source.member(company, item, user).await?;
    let approval = source.approval(company, item).await?;
    Ok(super::domain::project(item, member, approval, heads, authors, reviews))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
    }

    impl ExecutedCopySource for Fake {
        fn list<'a>(&'a self, company: &'a str, item: &'a str) -> LaneFuture<'a, Vec<CopyHead>> {
            let calls = &self.calls;
            Box::pin(async move {
                calls.lock().unwrap().push(format!("list:{company}:{item}"));
                Ok(Vec::new())
            })
        }
        fn authors<'a>(
            &'a self,
            _company: &'a str,
            _item: &'a str,
            _copy: &'a str,
        ) -> LaneFuture<'a, Option<CopyAuthors>> {
            Box::pin(async { Ok(None) })
        }
        fn reviews<'a>(
            &'a self,
            _company: &'a str,
            _copy: &'a str,
        ) -> LaneFuture<'a, Vec<(String, ReviewRow)>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn member<'a>(
            &'a self,
            company: &'a str,
            item: &'a str,
            user: &'a str,
        ) -> LaneFuture<'a, bool> {
            let calls = &self.calls;
            Box::pin(async move {
                calls.lock().unwrap().push(format!("member:{company}:{item}:{user}"));
                Ok(true)
            })
        }
        fn approval<'a>(
            &'a self,
            company: &'a str,
            item: &'a str,
        ) -> LaneFuture<'a, Option<i64>> {
            let calls = &self.calls;
            Box::pin(async move {
                calls.lock().unwrap().push(format!("approval:{company}:{item}"));
                Ok(Some(1))
            })
        }
    }

    #[tokio::test]
    async fn handler_binds_company_and_user_to_every_fact() {
        let fake = Fake::default();
        let read = read_executed_copies(&fake, "company-1", "user-1", "item-1")
            .await
            .expect("read");
        assert!(read.copies.is_empty());
        assert!(read.member);
        assert_eq!(read.approval, Some(super::super::domain::ApprovalCycle { cycle: 1 }));
        let calls = fake.calls.lock().unwrap();
        assert!(calls.contains(&"list:company-1:item-1".to_owned()));
        assert!(calls.contains(&"member:company-1:item-1:user-1".to_owned()));
        assert!(calls.contains(&"approval:company-1:item-1".to_owned()));
    }
}
