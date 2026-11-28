//! Owner-owned caller-unit read plans for Benefits attachments.
//!
//! Generated from the retired TypeScript sensitive-fact point reads with their
//! exact positional payloads. The mounted `benefits/command-plan` route
//! resolves the allow-listed kind here and returns the statement for the caller
//! to prepare and execute on its own open unit. Reads carry no
//! receipt/audit/outbox ledger. No SQL is ever accepted from the wire and no
//! TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the number
/// of positional arguments the caller must send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeBenefitReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript sensitive dependent point read. The
/// caller binds `[user_id, target_id, company_id]`.
pub const RD_ATTACHMENT_BENEFIT_DEPENDENTS: &str = "benefits_rd_attachment_benefit_dependents";
const SQL_ATTACHMENT_BENEFIT_DEPENDENTS: &str = r#"SELECT d.company_id,d.version,d.status,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,d.company_id) THEN 1 ELSE 0 END is_owner
      FROM benefit_dependents d
      JOIN hr_people p ON p.id=d.person_id AND p.company_id=d.company_id LEFT JOIN LATERAL (SELECT x.branch_id,x.department_id FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=p.company_id ORDER BY CASE WHEN x.status='active' THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1) e ON true
      WHERE d.id=? AND d.company_id=?"#;
const READ_ATTACHMENT_BENEFIT_DEPENDENTS: NativeBenefitReadStatement =
    NativeBenefitReadStatement {
        kind: RD_ATTACHMENT_BENEFIT_DEPENDENTS,
        sql: SQL_ATTACHMENT_BENEFIT_DEPENDENTS,
        bind_count: 3,
    };

/// Native successor of the TypeScript sensitive election point read. The
/// caller binds `[user_id, target_id, company_id]`.
pub const RD_ATTACHMENT_BENEFIT_ELECTIONS: &str = "benefits_rd_attachment_benefit_elections";
const SQL_ATTACHMENT_BENEFIT_ELECTIONS: &str = r#"SELECT x.company_id,x.version,x.status,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,x.company_id) THEN 1 ELSE 0 END is_owner
      FROM benefit_elections x
      JOIN hr_people p ON p.id=x.person_id AND p.company_id=x.company_id JOIN hr_engagements e ON e.id=x.engagement_id AND e.company_id=x.company_id
      WHERE x.id=? AND x.company_id=?"#;
const READ_ATTACHMENT_BENEFIT_ELECTIONS: NativeBenefitReadStatement = NativeBenefitReadStatement {
    kind: RD_ATTACHMENT_BENEFIT_ELECTIONS,
    sql: SQL_ATTACHMENT_BENEFIT_ELECTIONS,
    bind_count: 3,
};

const STATEMENTS: &[NativeBenefitReadStatement] = &[
    READ_ATTACHMENT_BENEFIT_DEPENDENTS,
    READ_ATTACHMENT_BENEFIT_ELECTIONS,
];

/// Resolve an allow-listed owner read kind. Any unregistered kind fails closed.
pub fn read_statement(kind: &str) -> Option<&'static NativeBenefitReadStatement> {
    STATEMENTS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependent_read_keeps_the_retired_literal_byte_for_byte() {
        let statement = read_statement(RD_ATTACHMENT_BENEFIT_DEPENDENTS)
            .expect("registered sensitive dependent read");
        assert_eq!(statement.kind, RD_ATTACHMENT_BENEFIT_DEPENDENTS);
        assert_eq!(
            statement.sql,
            concat!(
                "SELECT d.company_id,d.version,d.status,e.branch_id,e.department_id,\n",
                "        CASE WHEN iam_same_person(p.user_id,?,d.company_id) THEN 1 ELSE 0 END is_owner\n",
                "      FROM benefit_dependents d\n",
                "      JOIN hr_people p ON p.id=d.person_id AND p.company_id=d.company_id LEFT JOIN LATERAL (SELECT x.branch_id,x.department_id FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=p.company_id ORDER BY CASE WHEN x.status='active' THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1) e ON true\n",
                "      WHERE d.id=? AND d.company_id=?"
            )
        );
        assert_eq!(statement.bind_count, 3);
        assert_eq!(statement.sql.matches('?').count(), 3);
        assert!(statement.sql.starts_with("SELECT d.company_id,"));
        assert!(statement.sql.ends_with("WHERE d.id=? AND d.company_id=?"));
    }

    #[test]
    fn election_read_keeps_the_retired_literal_byte_for_byte() {
        let statement = read_statement(RD_ATTACHMENT_BENEFIT_ELECTIONS)
            .expect("registered sensitive election read");
        assert_eq!(statement.kind, RD_ATTACHMENT_BENEFIT_ELECTIONS);
        assert_eq!(
            statement.sql,
            concat!(
                "SELECT x.company_id,x.version,x.status,e.branch_id,e.department_id,\n",
                "        CASE WHEN iam_same_person(p.user_id,?,x.company_id) THEN 1 ELSE 0 END is_owner\n",
                "      FROM benefit_elections x\n",
                "      JOIN hr_people p ON p.id=x.person_id AND p.company_id=x.company_id JOIN hr_engagements e ON e.id=x.engagement_id AND e.company_id=x.company_id\n",
                "      WHERE x.id=? AND x.company_id=?"
            )
        );
        assert_eq!(statement.bind_count, 3);
        assert_eq!(statement.sql.matches('?').count(), 3);
        assert!(statement.sql.starts_with("SELECT x.company_id,"));
        assert!(statement.sql.ends_with("WHERE x.id=? AND x.company_id=?"));
    }

    #[test]
    fn owner_reads_are_select_only_without_locks_or_writes() {
        assert_eq!(STATEMENTS.len(), 2);
        for statement in STATEMENTS {
            let sql = statement.sql.to_ascii_lowercase();
            assert!(!statement.sql.contains(';'));
            assert!(!sql.contains("for update"));
            assert!(!sql.contains("for share"));
            assert!(!sql.contains("lock "));
            assert!(!sql.contains("insert "));
            assert!(!sql.contains("update "));
            assert!(!sql.contains("delete "));
        }
    }

    #[test]
    fn unadmitted_kinds_fail_closed() {
        for kind in [
            "benefits_rd_attachment_benefit_dependents_extra",
            "benefits_rd_attachment_benefit_plan",
            "benefits_rd_sensitive_facts",
            "attachments_sensitive_facts",
        ] {
            assert!(read_statement(kind).is_none(), "{kind} must stay unregistered");
        }
    }
}
