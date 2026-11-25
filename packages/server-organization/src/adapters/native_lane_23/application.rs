//! Owner application handler for the directory hierarchy read. The port is
//! injected; this layer contains no SQL, no pool and no HTTP details. It is the
//! single place where native facts are validated before they can be returned.
use super::domain::DirectoryHierarchy;
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type DirectoryHierarchyFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The Organization owner reader. Composition supplies the Pg implementation;
/// the router and tests can supply any bounded implementation of the same port.
pub trait DirectoryHierarchyRead: Send + Sync {
    fn read<'a>(&'a self) -> DirectoryHierarchyFuture<'a, DirectoryHierarchy>;
}

/// Execute the owner read and enforce its response invariants. No business
/// fallback exists: a failed or malformed native read propagates as-is.
pub async fn read_directory_hierarchy<R: DirectoryHierarchyRead + ?Sized>(
    reader: &R,
) -> Result<DirectoryHierarchy, BusinessError> {
    let facts = reader.read().await?;
    facts.validate()?;
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_23::domain::{
        DirectoryLevel, DirectoryNode, DirectoryPosition,
    };
    use erpx_server_shared::public::FailureKind;

    #[derive(Clone)]
    struct Stub {
        result: Result<DirectoryHierarchy, BusinessError>,
    }

    impl DirectoryHierarchyRead for Stub {
        fn read<'a>(&'a self) -> DirectoryHierarchyFuture<'a, DirectoryHierarchy> {
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    fn valid() -> DirectoryHierarchy {
        DirectoryHierarchy {
            nodes: vec![DirectoryNode {
                id: "n1".into(),
                company_id: "c1".into(),
                kind: "company".into(),
                slug: "acme".into(),
                display_name: "Acme".into(),
                parent_id: None,
                branch_id: None,
            }],
            levels: vec![DirectoryLevel {
                id: "l1".into(),
                company_id: "c1".into(),
                code: "L1".into(),
                name: "One".into(),
                level_number: Some(1),
            }],
            positions: vec![DirectoryPosition {
                id: "p1".into(),
                company_id: "c1".into(),
                branch_id: None,
                department_id: None,
                title: "CEO".into(),
                level_id: "l1".into(),
                reports_to_position_id: None,
                is_ceo: 1,
            }],
        }
    }

    #[tokio::test]
    async fn handler_returns_validated_native_facts() {
        let facts = read_directory_hierarchy(&Stub {
            result: Ok(valid()),
        })
        .await
        .unwrap();
        assert_eq!(facts.nodes[0].slug, "acme");
        assert_eq!(facts.positions[0].is_ceo, 1);
    }

    #[tokio::test]
    async fn handler_propagates_reader_failure() {
        let error = BusinessError::new(
            FailureKind::Unavailable,
            "NATIVE_READ_DOWN",
            "The native reader is unavailable.",
        );
        let result = read_directory_hierarchy(&Stub {
            result: Err(error.clone()),
        })
        .await;
        assert_eq!(result, Err(error));
    }

    #[tokio::test]
    async fn handler_rejects_malformed_native_facts() {
        let mut facts = valid();
        facts.levels[0].level_number = Some(99);
        let result = read_directory_hierarchy(&Stub { result: Ok(facts) }).await;
        assert_eq!(
            result,
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "ORGANIZATION_DIRECTORY_HIERARCHY_INVALID",
                "The native Organization directory hierarchy was invalid."
            ))
        );
    }
}
