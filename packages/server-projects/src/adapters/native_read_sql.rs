//! Owner-owned caller-unit read plans for Projects. Generated from the retired
//! TypeScript SELECT literals with their exact positional payload; the caller
//! resolves a closed kind here and composes the returned statement inside its
//! own transaction. A caller never supplies SQL or a relation name and no
//! TypeScript SQL remains for these kinds.
//!
//! Reads carry no receipt/audit/outbox ledger: the caller runs them on the same
//! unit as the surrounding work, so they observe uncommitted effects.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the
/// positional argument count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the r44 `final-root-owner-projection-sources` member
/// projection. Both closed final-root sources now delegate to the single owner
/// renderer `selectProjectionFrom` (read-tail-r40); this crate records their
/// exact byte-identical statements.
pub const RD_FINAL_ROOT_MEMBERS: &str = "projects_rd_final_root_members";
const SQL_FINAL_ROOT_MEMBERS: &str =
    "SELECT \"project_id\",\"user_id\",\"added_by\",\"created_at\" FROM staff_project_members";
const READ_FINAL_ROOT_MEMBERS: NativeReadStatement = NativeReadStatement {
    kind: RD_FINAL_ROOT_MEMBERS,
    sql: SQL_FINAL_ROOT_MEMBERS,
    bind_count: 0,
};

/// Native successor of the r44 final-root projects projection.
pub const RD_FINAL_ROOT_PROJECTS: &str = "projects_rd_final_root_projects";
const SQL_FINAL_ROOT_PROJECTS: &str =
    "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"description\",\"owner_user_id\",\"version\",\"created_at\",\"updated_at\",\"lifecycle_status\" FROM staff_projects";
const READ_FINAL_ROOT_PROJECTS: NativeReadStatement = NativeReadStatement {
    kind: RD_FINAL_ROOT_PROJECTS,
    sql: SQL_FINAL_ROOT_PROJECTS,
    bind_count: 0,
};

const READS: &[NativeReadStatement] = &[READ_FINAL_ROOT_MEMBERS, READ_FINAL_ROOT_PROJECTS];

/// Whether a kind is an allow-listed owner read (never a write command).
pub fn is_read(kind: &str) -> bool {
    read_statement(kind).is_some()
}

/// Resolve an owner read kind to its planned statement.
pub fn read_statement(kind: &str) -> Option<&'static NativeReadStatement> {
    READS.iter().find(|statement| statement.kind == kind)
}

/// Canonical owner form of the universal projection head retired from the
/// TypeScript read tail. The TS renderer `selectProjectionFrom` must reproduce
/// this exact prefix; this crate records the native literal.
fn projection_from_sql(columns: &str, relation: &str) -> String {
    format!("SELECT {columns} FROM {relation}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_plans_are_read_only_and_bounded() {
        for kind in [RD_FINAL_ROOT_MEMBERS, RD_FINAL_ROOT_PROJECTS] {
            let read = read_statement(kind).expect("read kind");
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"), "{kind}");
            assert!(!upper.contains("INSERT"), "{kind}");
            assert!(!upper.contains("UPDATE "), "{kind}");
            assert!(!upper.contains("DELETE"), "{kind}");
            assert!(!upper.contains("FOR UPDATE"), "{kind}");
            assert!(!upper.contains("LOCK TABLE"), "{kind}");
            assert_eq!(read.bind_count, 0, "{kind}");
            assert_eq!(read.sql.matches('?').count(), 0, "{kind}");
            assert!(is_read(kind));
        }
        assert!(!is_read("projects_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }

    #[test]
    fn final_root_statements_match_retired_typescript_literals() {
        assert_eq!(
            read_statement(RD_FINAL_ROOT_MEMBERS).expect("members").sql,
            "SELECT \"project_id\",\"user_id\",\"added_by\",\"created_at\" FROM staff_project_members"
        );
        assert_eq!(
            read_statement(RD_FINAL_ROOT_PROJECTS).expect("projects").sql,
            "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"description\",\"owner_user_id\",\"version\",\"created_at\",\"updated_at\",\"lifecycle_status\" FROM staff_projects"
        );
    }

    #[test]
    fn final_root_family_matches_the_owner_projection_renderer() {
        let cases = [
            (
                RD_FINAL_ROOT_MEMBERS,
                projection_from_sql(
                    "\"project_id\",\"user_id\",\"added_by\",\"created_at\"",
                    "staff_project_members",
                ),
                SQL_FINAL_ROOT_MEMBERS,
            ),
            (
                RD_FINAL_ROOT_PROJECTS,
                projection_from_sql(
                    "\"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"description\",\"owner_user_id\",\"version\",\"created_at\",\"updated_at\",\"lifecycle_status\"",
                    "staff_projects",
                ),
                SQL_FINAL_ROOT_PROJECTS,
            ),
        ];
        for (kind, rendered, expected) in cases {
            assert_eq!(rendered, expected, "{kind}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{kind}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{kind}");
            assert!(!upper.contains("UPDATE "), "{kind}");
            assert!(!upper.contains("DELETE"), "{kind}");
            assert!(!upper.contains("FOR UPDATE"), "{kind}");
            assert!(!upper.contains("LOCK TABLE"), "{kind}");
        }
    }
}
