//! Owner application handler for the directory hierarchy read. The port is
//! injected; this layer contains no SQL, no pool and no HTTP details. It is the
//! single place where native facts are validated before they can be returned.
use super::domain::{
    DirectoryHierarchy, ExecutiveEvidence, ExecutiveEvidenceKind, NodeReferences,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type DirectoryHierarchyFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The Organization owner reader. Composition supplies the Pg implementation;
/// the router and tests can supply any bounded implementation of the same port.
pub trait DirectoryHierarchyRead: Send + Sync {
    fn read<'a>(&'a self) -> DirectoryHierarchyFuture<'a, DirectoryHierarchy>;
}

/// Owner-local point references on `organization_nodes`, supplied through the
/// same holding-scoped port by the Pg adapter. No other owner's table is read.
pub trait NodeReferenceRead: Send + Sync {
    fn node_references<'a>(
        &'a self,
        company_id: &'a str,
        node_id: Option<&'a str>,
    ) -> DirectoryHierarchyFuture<'a, NodeReferences>;
}

/// SQL-free request for one bounded executive evidence source. Text binds stay
/// in exact SQL order; numeric limit/cursor binds are carried separately so the
/// owner adapter can bind them with their native Postgres types.
#[derive(Clone, Debug)]
pub struct ExecutiveEvidenceRequest {
    pub kind: ExecutiveEvidenceKind,
    pub company_id: String,
    pub values: Vec<Option<String>>,
    pub query: Option<String>,
    pub limit: Option<i64>,
    pub node_id: Option<String>,
    pub cursor_name: Option<String>,
    pub cursor_at: Option<i64>,
    pub cursor_id: Option<String>,
}

impl ExecutiveEvidenceRequest {
    /// Reject oversized or malformed transport before any Pg work is done.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.company_id.is_empty()
            || self.company_id.len() > 200
            || self.values.len() > 8
            || self.values.iter().flatten().any(|value| value.len() > 200)
            || self.query.as_deref().is_some_and(|query| query.len() > 200)
            || self.node_id.as_deref().is_some_and(|id| id.len() > 200)
            || self.cursor_name.as_deref().is_some_and(|name| name.len() > 200)
            || self.cursor_id.as_deref().is_some_and(|id| id.len() > 200)
            || self.limit.is_some_and(|limit| !(1..=500).contains(&limit))
            || (self.kind == ExecutiveEvidenceKind::NodeEvidence
                || self.kind == ExecutiveEvidenceKind::PendingEvidence)
                && self.node_id.is_some()
                && (self.limit.is_some() || self.query.is_some() || self.cursor_id.is_some())
        {
            return Err(BusinessError::invalid(
                "ORGANIZATION_EXECUTIVE_EVIDENCE_REQUEST_INVALID",
                "Use one bounded executive evidence read.",
            ));
        }
        Ok(())
    }
}

pub trait ExecutiveEvidenceRead: Send + Sync {
    fn evidence<'a>(
        &'a self,
        request: &'a ExecutiveEvidenceRequest,
    ) -> DirectoryHierarchyFuture<'a, ExecutiveEvidence>;
}

pub async fn read_executive_evidence<R: ExecutiveEvidenceRead + ?Sized>(
    reader: &R,
    request: &ExecutiveEvidenceRequest,
) -> Result<ExecutiveEvidence, BusinessError> {
    request.validate()?;
    let facts = reader.evidence(request).await?;
    facts.validate()?;
    Ok(facts)
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

/// Execute the bounded node-reference read and enforce its shape.
pub async fn read_node_references<R: NodeReferenceRead + ?Sized>(
    reader: &R,
    company_id: &str,
    node_id: Option<&str>,
) -> Result<NodeReferences, BusinessError> {
    let facts = reader.node_references(company_id, node_id).await?;
    facts.validate()?;
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_23::domain::{
        AttachmentNodeFact, AudienceNode, DirectoryLevel, DirectoryNode, DirectoryPosition,
        NodeReferences, UnitReference,
    };
    use erpx_platform_shared_server::public::FailureKind;

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

    #[derive(Clone)]
    struct RefStub {
        result: Result<NodeReferences, BusinessError>,
    }

    impl NodeReferenceRead for RefStub {
        fn node_references<'a>(
            &'a self,
            _: &'a str,
            _: Option<&'a str>,
        ) -> DirectoryHierarchyFuture<'a, NodeReferences> {
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    fn references() -> NodeReferences {
        NodeReferences {
            node: Some(AudienceNode {
                id: "n1".into(),
                kind: "branch".into(),
                branch_id: None,
            }),
            units: vec![UnitReference {
                id: "n1".into(),
                name: "Branch".into(),
                kind: "branch".into(),
                branch_id: None,
            }],
            attachment: Some(AttachmentNodeFact {
                company_id: "c1".into(),
                branch_id: None,
                department_id: None,
                version: 1,
                kind: "branch".into(),
            }),
        }
    }

    #[derive(Clone)]
    struct EvidenceStub {
        rows: Result<Vec<serde_json::Value>, BusinessError>,
    }

    impl ExecutiveEvidenceRead for EvidenceStub {
        fn evidence<'a>(
            &'a self,
            _: &'a ExecutiveEvidenceRequest,
        ) -> DirectoryHierarchyFuture<'a, ExecutiveEvidence> {
            let rows = self.rows.clone();
            Box::pin(async move { rows.map(|rows| ExecutiveEvidence { rows }) })
        }
    }

    fn evidence_request() -> ExecutiveEvidenceRequest {
        ExecutiveEvidenceRequest {
            kind: ExecutiveEvidenceKind::ConnectionScopes,
            company_id: "c1".into(),
            values: vec![None, Some("n1".into()), None, None, None],
            query: None,
            limit: None,
            node_id: None,
            cursor_name: None,
            cursor_at: None,
            cursor_id: None,
        }
    }

    #[tokio::test]
    async fn executive_evidence_handler_validates_bounds_and_rows() {
        let facts = read_executive_evidence(
            &EvidenceStub {
                rows: Ok(vec![serde_json::json!({"id":"n1"})]),
            },
            &evidence_request(),
        )
        .await
        .unwrap();
        assert_eq!(facts.rows.len(), 1);
        let mut request = evidence_request();
        request.limit = Some(501);
        assert!(read_executive_evidence(
            &EvidenceStub { rows: Ok(vec![]) },
            &request,
        )
        .await
        .is_err());
        let bad = read_executive_evidence(
            &EvidenceStub {
                rows: Ok(vec![serde_json::json!({"kind":"branch"})]),
            },
            &evidence_request(),
        )
        .await;
        assert!(bad.is_err());
    }

    #[tokio::test]
    async fn node_reference_handler_validates_and_propagates() {
        let facts = read_node_references(
            &RefStub {
                result: Ok(references()),
            },
            "c1",
            Some("n1"),
        )
        .await
        .unwrap();
        assert_eq!(facts.node.unwrap().id, "n1");
        let mut bad = references();
        bad.attachment.as_mut().unwrap().version = 0;
        assert!(read_node_references(&RefStub { result: Ok(bad) }, "c1", None)
            .await
            .is_err());
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
