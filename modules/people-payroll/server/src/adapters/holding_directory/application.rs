//! SQL-free application for the native Holding people directory. The reader
//! port is owner-local; Account authority and Company selection are supplied by
//! composition and carried in `HoldingDirectoryScope`.

use super::domain::{
    DirectoryCompany, DirectoryLevel, DirectoryNode, DirectoryPerson, DirectoryPosition,
    HoldingDirectoryFacts, HoldingDirectoryScope, HoldingPeopleDirectory, assignment_forbidden,
    scope_required, scoped,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type HoldingDirectoryFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner read port. Composition binds the Pg implementation; no SQL and no
/// authority decision lives in the transport or in composition.
pub trait HoldingDirectoryRead: Send + Sync {
    fn read<'a>(
        &'a self,
        scope: &'a HoldingDirectoryScope,
    ) -> HoldingDirectoryFuture<'a, HoldingDirectoryFacts>;
}

/// Port through which composition supplies verified Account authority. The
/// router only transports the Holding target and browser context headers.
pub trait HoldingDirectoryAuthority: Send + Sync {
    fn work_scope<'a>(
        &'a self,
        host: &'a str,
        holding_slug: &'a str,
        headers: &'a erpx_platform_runtime_server::public::http_support::HeaderMap,
    ) -> HoldingDirectoryFuture<'a, HoldingDirectoryScope>;
}

/// The retired `createHoldingDirectoryApplication().directory()` authorization
/// and projection, now entirely owner-local and SQL-free.
pub fn read_holding_directory<'a>(
    reader: &'a dyn HoldingDirectoryRead,
    scope: &'a HoldingDirectoryScope,
) -> HoldingDirectoryFuture<'a, HoldingPeopleDirectory> {
    Box::pin(async move {
        if !scoped(scope) || !super::domain::valid_assignment_scope(&scope.assignment_scope) {
            return Err(scope_required());
        }
        let facts = reader.read(scope).await?;
        if !facts.assignment_active {
            return Err(assignment_forbidden());
        }
        Ok(HoldingPeopleDirectory {
            companies: facts
                .companies
                .into_iter()
                .map(|row| DirectoryCompany {
                    id: row.id,
                    slug: row.slug,
                    name: row.name,
                })
                .collect(),
            nodes: facts
                .nodes
                .into_iter()
                .map(|row| DirectoryNode {
                    id: row.id,
                    company_id: row.company_id,
                    kind: row.kind,
                    slug: row.slug,
                    name: row.name,
                    parent_id: row.parent_id,
                    branch_id: row.branch_id,
                })
                .collect(),
            levels: facts
                .levels
                .into_iter()
                .map(|row| DirectoryLevel {
                    id: row.id,
                    company_id: row.company_id,
                    code: row.code,
                    name: row.name,
                    number: row.number,
                })
                .collect(),
            positions: facts
                .positions
                .into_iter()
                .map(|row| DirectoryPosition {
                    id: row.id,
                    company_id: row.company_id,
                    branch_id: row.branch_id,
                    department_id: row.department_id,
                    title: row.title,
                    level_id: row.level_id,
                    reports_to_position_id: row.reports_to_position_id,
                    is_ceo: row.is_ceo,
                })
                .collect(),
            people: facts
                .people
                .into_iter()
                .map(|row| DirectoryPerson {
                    id: row.engagement_id.clone(),
                    person_id: row.person_id,
                    user_id: row.user_id,
                    engagement_id: row.engagement_id,
                    company_id: row.company_id,
                    branch_id: row.branch_id,
                    department_id: row.department_id,
                    position_id: row.position_id,
                    legal_name: row.legal_name,
                    job_title: row.job_title,
                    work_email: row.work_email,
                    work_phone: row.work_phone,
                })
                .collect(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Reader {
        active: bool,
    }

    impl HoldingDirectoryRead for Reader {
        fn read<'a>(
            &'a self,
            _: &'a HoldingDirectoryScope,
        ) -> HoldingDirectoryFuture<'a, HoldingDirectoryFacts> {
            let active = self.active;
            Box::pin(async move {
                Ok(HoldingDirectoryFacts {
                    assignment_active: active,
                    companies: vec![DirectoryCompany {
                        id: "c1".into(),
                        slug: "acme".into(),
                        name: "Acme".into(),
                    }],
                    nodes: vec![DirectoryNode {
                        id: "n1".into(),
                        company_id: "c1".into(),
                        kind: "company".into(),
                        slug: "acme".into(),
                        name: "Acme HQ".into(),
                        parent_id: None,
                        branch_id: None,
                    }],
                    levels: vec![DirectoryLevel {
                        id: "l1".into(),
                        company_id: "c1".into(),
                        code: "L1".into(),
                        name: "One".into(),
                        number: Some(1),
                    }],
                    positions: vec![DirectoryPosition {
                        id: "p1".into(),
                        company_id: "c1".into(),
                        branch_id: None,
                        department_id: None,
                        title: "CEO".into(),
                        level_id: "l1".into(),
                        reports_to_position_id: None,
                        is_ceo: true,
                    }],
                    people: vec![DirectoryPerson {
                        id: "e1".into(),
                        person_id: "person-1".into(),
                        user_id: Some("user-1".into()),
                        engagement_id: "e1".into(),
                        company_id: "c1".into(),
                        branch_id: None,
                        department_id: None,
                        position_id: Some("p1".into()),
                        legal_name: "Ada Lovelace".into(),
                        job_title: "CEO".into(),
                        work_email: Some("ada@example.test".into()),
                        work_phone: None,
                    }],
                })
            })
        }
    }

    fn scope() -> HoldingDirectoryScope {
        HoldingDirectoryScope {
            holding_id: "h1".into(),
            binding_id: "h1".into(),
            company_id: "c1".into(),
            scope_holding_id: Some("h1".into()),
            scope_company_id: Some("c1".into()),
            assignment_id: "a1".into(),
            user_id: "user-1".into(),
            role: "holding_owner".into(),
            assignment_scope: "holding".into(),
        }
    }

    #[tokio::test]
    async fn query_projects_every_retired_directory_field() {
        let reader = Reader { active: true };
        let data = read_holding_directory(&reader, &scope()).await.unwrap();
        assert_eq!(
            serde_json::to_value(&data).unwrap(),
            json!({
                "companies": [{"id": "c1", "slug": "acme", "name": "Acme"}],
                "nodes": [{"id": "n1", "companyId": "c1", "kind": "company", "slug": "acme", "name": "Acme HQ", "parentId": null, "branchId": null}],
                "levels": [{"id": "l1", "companyId": "c1", "code": "L1", "name": "One", "number": 1}],
                "positions": [{"id": "p1", "companyId": "c1", "branchId": null, "departmentId": null, "title": "CEO", "levelId": "l1", "reportsToPositionId": null, "isCeo": true}],
                "people": [{"id": "e1", "personId": "person-1", "userId": "user-1", "engagementId": "e1", "companyId": "c1", "branchId": null, "departmentId": null, "positionId": "p1", "legalName": "Ada Lovelace", "jobTitle": "CEO", "workEmail": "ada@example.test", "workPhone": null}]
            })
        );
    }

    #[tokio::test]
    async fn query_enforces_scope_and_active_assignment() {
        let reader = Reader { active: true };
        let mut foreign = scope();
        foreign.scope_company_id = Some("c2".into());
        assert_eq!(
            read_holding_directory(&reader, &foreign)
                .await
                .unwrap_err()
                .code,
            "HOLDING_SCOPE_REQUIRED"
        );
        let inactive = Reader { active: false };
        assert_eq!(
            read_holding_directory(&inactive, &scope())
                .await
                .unwrap_err()
                .code,
            "HOLDING_DIRECTORY_FORBIDDEN"
        );
    }
}
