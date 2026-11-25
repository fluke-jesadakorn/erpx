//! Owner-owned caller-unit read plans for Manage ERPX. Generated from the
//! retired TypeScript `beta_policy` point-read literal with the exact
//! positional payload. The mounted `/manage/command-plan` route resolves the
//! allow-listed kind here and returns the statement for the caller to prepare
//! and execute on its own open unit.
//!
//! The three beta_policy kinds are the closed projection union admitted by
//! the owner helper: `*`, `member_limit` and `branch_limit`. The four
//! point-by-key kinds are the byte-identical successors of the retired
//! `beta_slots` and `beta_team_invitations` single-row reads. The Company
//! provisioning kind is the byte-identical successor of the retired private
//! attachment-owner source image read. No other kind or SQL text can cross the
//! wire. Reads carry no receipt/audit/outbox ledger.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the number
/// of positional arguments the caller must send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeManageReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `betaPolicyPointSql('*')` point read.
/// The caller binds `[company_id]`.
pub const ME_RD_BETA_POLICY_POINT_ALL: &str = "manage_rd_beta_policy_point_all";
const SQL_BETA_POLICY_POINT_ALL: &str = "SELECT * FROM beta_policy WHERE company_id=?";
const READ_BETA_POLICY_POINT_ALL: NativeManageReadStatement = NativeManageReadStatement {
    kind: ME_RD_BETA_POLICY_POINT_ALL,
    sql: SQL_BETA_POLICY_POINT_ALL,
    bind_count: 1,
};

/// Native successor of the TypeScript `betaPolicyPointSql('member_limit')`
/// point read. The caller binds `[company_id]`.
pub const ME_RD_BETA_POLICY_POINT_MEMBER: &str = "manage_rd_beta_policy_point_member_limit";
const SQL_BETA_POLICY_POINT_MEMBER: &str =
    "SELECT member_limit FROM beta_policy WHERE company_id=?";
const READ_BETA_POLICY_POINT_MEMBER: NativeManageReadStatement = NativeManageReadStatement {
    kind: ME_RD_BETA_POLICY_POINT_MEMBER,
    sql: SQL_BETA_POLICY_POINT_MEMBER,
    bind_count: 1,
};

/// Native successor of the TypeScript `betaPolicyPointSql('branch_limit')`
/// point read. The caller binds `[company_id]`.
pub const ME_RD_BETA_POLICY_POINT_BRANCH: &str = "manage_rd_beta_policy_point_branch_limit";
const SQL_BETA_POLICY_POINT_BRANCH: &str =
    "SELECT branch_limit FROM beta_policy WHERE company_id=?";
const READ_BETA_POLICY_POINT_BRANCH: NativeManageReadStatement = NativeManageReadStatement {
    kind: ME_RD_BETA_POLICY_POINT_BRANCH,
    sql: SQL_BETA_POLICY_POINT_BRANCH,
    bind_count: 1,
};

/// Native successor of the TypeScript `runtimeBetaSlotActive` probe. The
/// caller binds `[holding_id]`; the active-state predicate is owner-owned.
pub const ME_RD_SLOT_ACTIVE: &str = "manage_rd_slot_active";
const SQL_SLOT_ACTIVE: &str = "SELECT 1 FROM beta_slots WHERE holding_id=? AND state='active'";
const READ_SLOT_ACTIVE: NativeManageReadStatement = NativeManageReadStatement {
    kind: ME_RD_SLOT_ACTIVE,
    sql: SQL_SLOT_ACTIVE,
    bind_count: 1,
};

/// Native successor of the TypeScript `ownerOnboardingSlot` entrypoint read.
/// The caller binds `[owner_user_id]`.
pub const ME_RD_SLOT_OWNER: &str = "manage_rd_slot_owner";
const SQL_SLOT_OWNER: &str =
    "SELECT slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=?";
const READ_SLOT_OWNER: NativeManageReadStatement = NativeManageReadStatement {
    kind: ME_RD_SLOT_OWNER,
    sql: SQL_SLOT_OWNER,
    bind_count: 1,
};

/// Native successor of the TypeScript `beta-team` invitation token lookup.
/// The caller binds `[token_hash]`.
pub const ME_RD_INVITATION_BY_TOKEN: &str = "manage_rd_invitation_by_token";
const SQL_INVITATION_BY_TOKEN: &str =
    "SELECT * FROM beta_team_invitations WHERE token_hash=?";
const READ_INVITATION_BY_TOKEN: NativeManageReadStatement = NativeManageReadStatement {
    kind: ME_RD_INVITATION_BY_TOKEN,
    sql: SQL_INVITATION_BY_TOKEN,
    bind_count: 1,
};

/// Native successor of the TypeScript `beta-team` accepted-user lookup. The
/// caller binds `[id]`.
pub const ME_RD_INVITATION_ACCEPTED_USER: &str = "manage_rd_invitation_accepted_user";
const SQL_INVITATION_ACCEPTED_USER: &str =
    "SELECT accepted_user_id FROM beta_team_invitations WHERE id=?";
const READ_INVITATION_ACCEPTED_USER: NativeManageReadStatement = NativeManageReadStatement {
    kind: ME_RD_INVITATION_ACCEPTED_USER,
    sql: SQL_INVITATION_ACCEPTED_USER,
    bind_count: 1,
};

/// Native successor of the TypeScript `companyProvisioningAttachmentReadSourceImage`
/// read. The caller binds `[target_id, company_id]`; the duplicated Company
/// predicate is retained byte for byte.
pub const ME_RD_COMPANY_PROVISIONING_ATTACHMENT_IMAGE: &str =
    "manage_rd_company_provisioning_attachment_image";
const SQL_COMPANY_PROVISIONING_ATTACHMENT_IMAGE: &str = "SELECT jsonb_build_object('provisioning',to_jsonb(provisioning),'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM company_provisioning_events event WHERE event.company_id=provisioning.company_id AND event.holding_id=provisioning.holding_id))::text image FROM company_provisioning provisioning WHERE provisioning.company_id=? AND provisioning.company_id=?";
const READ_COMPANY_PROVISIONING_ATTACHMENT_IMAGE: NativeManageReadStatement =
    NativeManageReadStatement {
        kind: ME_RD_COMPANY_PROVISIONING_ATTACHMENT_IMAGE,
        sql: SQL_COMPANY_PROVISIONING_ATTACHMENT_IMAGE,
        bind_count: 2,
    };

const STATEMENTS: &[NativeManageReadStatement] = &[
    READ_BETA_POLICY_POINT_ALL,
    READ_BETA_POLICY_POINT_MEMBER,
    READ_BETA_POLICY_POINT_BRANCH,
    READ_SLOT_ACTIVE,
    READ_SLOT_OWNER,
    READ_INVITATION_BY_TOKEN,
    READ_INVITATION_ACCEPTED_USER,
    READ_COMPANY_PROVISIONING_ATTACHMENT_IMAGE,
];

/// Resolve an allow-listed owner read kind. Any unregistered kind fails closed.
pub fn read_statement(kind: &str) -> Option<&'static NativeManageReadStatement> {
    STATEMENTS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_point_reads_render_the_retired_literals_byte_for_byte() {
        let expected = [
            (ME_RD_BETA_POLICY_POINT_ALL, SQL_BETA_POLICY_POINT_ALL),
            (ME_RD_BETA_POLICY_POINT_MEMBER, SQL_BETA_POLICY_POINT_MEMBER),
            (ME_RD_BETA_POLICY_POINT_BRANCH, SQL_BETA_POLICY_POINT_BRANCH),
        ];
        for (kind, sql) in expected {
            let statement = read_statement(kind).expect("registered beta_policy point read");
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, 1);
            assert_eq!(sql.matches('?').count(), 1);
            assert!(sql.starts_with("SELECT "));
            assert!(sql.ends_with(" FROM beta_policy WHERE company_id=?"));
        }
        assert_eq!(STATEMENTS.len(), 8);
        assert_eq!(
            SQL_BETA_POLICY_POINT_ALL,
            "SELECT * FROM beta_policy WHERE company_id=?"
        );
        assert_eq!(
            SQL_BETA_POLICY_POINT_MEMBER,
            "SELECT member_limit FROM beta_policy WHERE company_id=?"
        );
        assert_eq!(
            SQL_BETA_POLICY_POINT_BRANCH,
            "SELECT branch_limit FROM beta_policy WHERE company_id=?"
        );
    }

    #[test]
    fn point_by_key_reads_render_the_retired_literals_byte_for_byte() {
        let expected = [
            (
                ME_RD_SLOT_ACTIVE,
                "SELECT 1 FROM beta_slots WHERE holding_id=? AND state='active'",
            ),
            (
                ME_RD_SLOT_OWNER,
                "SELECT slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=?",
            ),
            (
                ME_RD_INVITATION_BY_TOKEN,
                "SELECT * FROM beta_team_invitations WHERE token_hash=?",
            ),
            (
                ME_RD_INVITATION_ACCEPTED_USER,
                "SELECT accepted_user_id FROM beta_team_invitations WHERE id=?",
            ),
        ];
        for (kind, sql) in expected {
            let statement = read_statement(kind).expect("registered point-by-key read");
            assert_eq!(statement.kind, kind);
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, 1);
            assert_eq!(sql.matches('?').count(), 1);
            assert!(sql.starts_with("SELECT "));
        }
    }

    #[test]
    fn point_by_key_reads_are_select_only_without_locks_or_writes() {
        for kind in [
            ME_RD_SLOT_ACTIVE,
            ME_RD_SLOT_OWNER,
            ME_RD_INVITATION_BY_TOKEN,
            ME_RD_INVITATION_ACCEPTED_USER,
        ] {
            let statement = read_statement(kind).expect("registered point-by-key read");
            let sql = statement.sql.to_ascii_lowercase();
            assert!(!sql.contains(';'));
            assert!(!sql.contains("for update"));
            assert!(!sql.contains("lock "));
            assert!(!sql.contains("insert "));
            assert!(!sql.contains("update "));
            assert!(!sql.contains("delete "));
        }
    }

    #[test]
    fn company_provisioning_attachment_image_renders_the_retired_literal_byte_for_byte() {
        let statement = read_statement(ME_RD_COMPANY_PROVISIONING_ATTACHMENT_IMAGE)
            .expect("registered Company provisioning attachment image read");
        assert_eq!(statement.kind, ME_RD_COMPANY_PROVISIONING_ATTACHMENT_IMAGE);
        assert_eq!(statement.sql, SQL_COMPANY_PROVISIONING_ATTACHMENT_IMAGE);
        assert_eq!(statement.bind_count, 2);
        assert_eq!(statement.sql.matches('?').count(), 2);
        assert_eq!(statement.sql.len(), 385);
        assert!(statement.sql.starts_with("SELECT jsonb_build_object("));
        assert!(statement.sql.ends_with(
            "FROM company_provisioning provisioning WHERE provisioning.company_id=? AND provisioning.company_id=?"
        ));
    }

    #[test]
    fn unadmitted_projections_fail_closed() {
        for kind in [
            "manage_rd_beta_policy_point_ai_monthly_limit",
            "manage_rd_beta_policy_point",
            "beta_policy_point_select",
            "manage_rd_slot_active_extra",
            "manage_rd_invitation_by_email",
            "manage_rd_company_provisioning_attachment_image_extra",
            "manage_rd_company_provisioning_attachment_images",
        ] {
            assert!(read_statement(kind).is_none(), "{kind} must stay unregistered");
        }
    }

    #[test]
    fn owner_reads_are_select_only_without_locks_or_writes() {
        for statement in STATEMENTS {
            assert!(!statement.sql.contains(';'));
            assert!(!statement.sql.to_ascii_lowercase().contains("lock "));
            assert!(!statement.sql.to_ascii_lowercase().contains("for update"));
            assert!(!statement.sql.to_ascii_lowercase().contains("insert "));
            assert!(!statement.sql.to_ascii_lowercase().contains("update "));
            assert!(!statement.sql.to_ascii_lowercase().contains("delete "));
        }
    }
}
