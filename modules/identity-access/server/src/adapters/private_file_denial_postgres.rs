//! Owner-local PostgreSQL execution for the private-file denial authority
//! witness. The statement is the byte-for-byte successor of the deleted
//! TypeScript `private-file-denial-authority.ts` image query: only the
//! placeholder syntax changed (`?` to `$n`). It reads the closed IAM,
//! Organization and People projections of one Company; no row is mutated and
//! no caller SQL is accepted.
use crate::application::private_file_denial::{
    PrivateFileDenialFuture, PrivateFileDenialRepository,
};
use crate::domain::private_file_denial::PrivateFileDenialActor;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// The single closed image statement, with `$n` placeholders.
pub const SQL_PRIVATE_FILE_DENIAL_IMAGE: &str = r##"WITH RECURSIVE denial_scope AS(
 SELECT $1::text company_id,$2::text user_id,$3::text assignment_id,$4::text role_id,$5::text session_id),
chain AS(
 SELECT assignment.*,ARRAY[assignment.id] visited FROM assignments assignment CROSS JOIN denial_scope WHERE assignment.id=denial_scope.assignment_id
 UNION ALL SELECT parent.*,child.visited||parent.id FROM assignments parent JOIN chain child ON parent.id=child.parent_id WHERE NOT parent.id=ANY(child.visited) AND cardinality(child.visited)<32),
principals AS(SELECT company_id,user_id FROM chain UNION SELECT company_id,user_id FROM denial_scope),
members AS(SELECT member.* FROM iam_members member WHERE EXISTS(SELECT 1 FROM principals principal WHERE principal.company_id=member.company_id AND (principal.user_id=member.user_id OR member.person_id IS NOT NULL AND EXISTS(SELECT 1 FROM iam_members selected WHERE selected.company_id=principal.company_id AND selected.user_id=principal.user_id AND selected.person_id=member.person_id)))),
roles AS(SELECT role.* FROM iam_roles role WHERE EXISTS(SELECT 1 FROM chain WHERE chain.role=role.id AND role.company_id IN ('*',chain.company_id)) OR EXISTS(SELECT 1 FROM denial_scope WHERE role.id=denial_scope.role_id AND role.company_id IN ('*',denial_scope.company_id)))
SELECT jsonb_build_object(
'chain',(SELECT jsonb_agg(to_jsonb(chain) ORDER BY chain.id) FROM chain),
'members',(SELECT jsonb_agg(to_jsonb(member) ORDER BY member.company_id,member.user_id) FROM members member),
'identities',(SELECT jsonb_agg(to_jsonb(identity_row) ORDER BY identity_row.id) FROM iam_identities identity_row WHERE EXISTS(SELECT 1 FROM members member WHERE member.identity_id=identity_row.id AND member.company_id=identity_row.company_id)),
'connections',(SELECT jsonb_agg(to_jsonb(connection) ORDER BY connection.id) FROM iam_connections connection WHERE EXISTS(SELECT 1 FROM members member WHERE member.connection_id=connection.id AND member.company_id=connection.company_id)),
'roles',(SELECT jsonb_agg(to_jsonb(role) ORDER BY role.company_id,role.id) FROM roles role),
'revisions',(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.company_id,revision.role_id,revision.revision) FROM iam_role_revisions revision JOIN roles role ON role.company_id=revision.company_id AND role.id=revision.role_id AND role.published_revision=revision.revision),
'permissions',(SELECT jsonb_agg(to_jsonb(permission) ORDER BY permission.id) FROM iam_permissions permission),
'revocations',(SELECT jsonb_agg(to_jsonb(revocation) ORDER BY to_jsonb(revocation)::text) FROM iam_revoked_sessions revocation WHERE revocation.company_id=denial_scope.company_id AND revocation.user_id=denial_scope.user_id AND (denial_scope.session_id IS NULL OR revocation.session_id=denial_scope.session_id)),
'emergency',(SELECT jsonb_agg(to_jsonb(emergency) ORDER BY to_jsonb(emergency)::text) FROM iam_emergency_sessions emergency WHERE emergency.company_id=denial_scope.company_id AND emergency.user_id=denial_scope.user_id AND (denial_scope.session_id IS NULL OR emergency.session_id=denial_scope.session_id)),
'contexts',(SELECT jsonb_agg(to_jsonb(context_row) ORDER BY context_row.session_id) FROM active_work_contexts context_row WHERE context_row.company_id=denial_scope.company_id AND context_row.user_id=denial_scope.user_id AND (denial_scope.session_id IS NULL OR context_row.session_id=denial_scope.session_id)),
'subjects',(SELECT jsonb_agg(to_jsonb(subject) ORDER BY to_jsonb(subject)::text) FROM iam_access_subjects subject WHERE subject.company_id=denial_scope.company_id AND (subject.user_id=denial_scope.user_id OR subject.person_id IN(SELECT member.person_id FROM members member WHERE member.company_id=subject.company_id))),
'attributes',(SELECT jsonb_agg(to_jsonb(attribute) ORDER BY attribute.id) FROM um_attributes attribute WHERE attribute.company_id=denial_scope.company_id),
'values',(SELECT jsonb_agg(to_jsonb(value_row) ORDER BY value_row.target,value_row.object_id) FROM um_values value_row WHERE value_row.company_id=denial_scope.company_id),
'groups',(SELECT jsonb_agg(to_jsonb(group_row) ORDER BY group_row.id) FROM um_groups group_row WHERE group_row.company_id=denial_scope.company_id),
'memberships',(SELECT jsonb_agg(to_jsonb(membership) ORDER BY membership.group_id,membership.person_id) FROM um_memberships membership WHERE membership.company_id=denial_scope.company_id),
'policies',(SELECT jsonb_agg(to_jsonb(policy) ORDER BY policy.id) FROM um_policies policy WHERE policy.company_id=denial_scope.company_id),
'attachments',(SELECT jsonb_agg(to_jsonb(attachment) ORDER BY attachment.id) FROM um_attachments attachment WHERE attachment.company_id=denial_scope.company_id),
'candidates',(SELECT jsonb_agg(to_jsonb(candidate) ORDER BY candidate.id) FROM um_candidates candidate WHERE candidate.company_id=denial_scope.company_id),
'organization',jsonb_build_object(
 'nodes',(SELECT jsonb_agg(to_jsonb(node) ORDER BY node.id) FROM organization_nodes node WHERE node.company_id=denial_scope.company_id),
 'positions',(SELECT jsonb_agg(to_jsonb(position) ORDER BY position.id) FROM organization_positions position WHERE position.company_id=denial_scope.company_id),
 'seats',(SELECT jsonb_agg(to_jsonb(seat) ORDER BY seat.kind,seat.scope_id) FROM authority_seats seat WHERE seat.company_id=denial_scope.company_id)),
'people',jsonb_build_object(
 'people',(SELECT jsonb_agg(to_jsonb(person) ORDER BY person.id) FROM hr_people person WHERE person.company_id=denial_scope.company_id),
 'engagements',(SELECT jsonb_agg(to_jsonb(engagement) ORDER BY engagement.id) FROM hr_engagements engagement WHERE engagement.company_id=denial_scope.company_id),
 'cutoffs',(SELECT jsonb_agg(to_jsonb(cutoff) ORDER BY to_jsonb(cutoff)::text) FROM people_access_cutoffs cutoff WHERE cutoff.company_id=denial_scope.company_id),
 'cases',(SELECT jsonb_agg(to_jsonb(lifecycle) ORDER BY lifecycle.id) FROM people_lifecycle_cases lifecycle WHERE lifecycle.company_id=denial_scope.company_id),
 'grants',(SELECT jsonb_agg(to_jsonb(grant_row) ORDER BY to_jsonb(grant_row)::text) FROM people_lifecycle_grants grant_row WHERE grant_row.company_id=denial_scope.company_id),
 'packageRevisions',(SELECT jsonb_agg(to_jsonb(revision) ORDER BY to_jsonb(revision)::text) FROM people_package_revisions revision WHERE revision.company_id=denial_scope.company_id),
 'appliedPackages',(SELECT jsonb_agg(to_jsonb(package_row) ORDER BY to_jsonb(package_row)::text) FROM people_applied_packages package_row WHERE package_row.company_id=denial_scope.company_id),
 'provisionOperations',(SELECT jsonb_agg(to_jsonb(operation) ORDER BY to_jsonb(operation)::text) FROM people_provision_operations operation WHERE operation.company_id=denial_scope.company_id),
 'provisionGrants',(SELECT jsonb_agg(to_jsonb(provision) ORDER BY to_jsonb(provision)::text) FROM people_provision_grants provision WHERE EXISTS(SELECT 1 FROM people_provision_operations operation WHERE operation.id=provision.operation_id AND operation.company_id=denial_scope.company_id)))
)::text image FROM denial_scope"##;

/// The pre-migration TypeScript statement (`?` placeholders), kept only as a
/// byte-identity oracle for the retirement test. It is never executed.
pub const TS_ORACLE_SQL: &str = r##"WITH RECURSIVE denial_scope AS(SELECT ?::text company_id,?::text user_id,?::text assignment_id,?::text role_id,?::text session_id)"##;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PRIVATE_FILE_DENIAL_READ_UNAVAILABLE",
        "Private-file denial evidence is unavailable.",
    )
}

fn persistence(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub struct PgPrivateFileDenial {
    holding: DatabasePool,
}

impl PgPrivateFileDenial {
    pub fn new(holding: DatabasePool) -> Self {
        Self { holding }
    }
}

impl PrivateFileDenialRepository for PgPrivateFileDenial {
    type Error = BusinessError;

    fn image<'a>(
        &'a self,
        actor: &'a PrivateFileDenialActor,
    ) -> PrivateFileDenialFuture<'a, String, Self::Error> {
        let unit = PgUnitOfWork::new(self.holding.clone());
        let company_id = actor.company_id.clone();
        let user_id = actor.user_id.clone();
        let assignment_id = actor.assignment_id.clone();
        let role = actor.role.clone();
        let session_id = actor.session_id.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company_id = company_id.clone();
                let user_id = user_id.clone();
                let assignment_id = assignment_id.clone();
                let role = role.clone();
                let session_id = session_id.clone();
                Box::pin(async move {
                    let image = sqlx::query_scalar::<_, String>(SQL_PRIVATE_FILE_DENIAL_IMAGE)
                        .bind(company_id)
                        .bind(user_id)
                        .bind(assignment_id)
                        .bind(role)
                        .bind(session_id)
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(image)
                })
            })
            .await
            .map_err(persistence)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owner_statement_uses_only_bound_parameters() {
        assert!(SQL_PRIVATE_FILE_DENIAL_IMAGE.starts_with("WITH RECURSIVE denial_scope AS"));
        for index in 1..=5 {
            assert!(SQL_PRIVATE_FILE_DENIAL_IMAGE.contains(&format!("${index}")));
        }
        assert!(!SQL_PRIVATE_FILE_DENIAL_IMAGE.contains('?'));
        assert!(!SQL_PRIVATE_FILE_DENIAL_IMAGE.contains("INSERT INTO"));
        assert!(!SQL_PRIVATE_FILE_DENIAL_IMAGE.contains("UPDATE "));
        assert!(!SQL_PRIVATE_FILE_DENIAL_IMAGE.contains("DELETE FROM"));
        assert!(SQL_PRIVATE_FILE_DENIAL_IMAGE.contains("'organization',jsonb_build_object("));
        assert!(SQL_PRIVATE_FILE_DENIAL_IMAGE.contains("'people',jsonb_build_object("));
        assert!(TS_ORACLE_SQL.contains('?'));
    }
}
