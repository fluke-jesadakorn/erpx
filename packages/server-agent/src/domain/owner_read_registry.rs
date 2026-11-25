//! Pure registry for native Agent caller-unit reads.
//!
//! The owner owns the exact static SELECT bytes; the mounted agent command-plan
//! route returns them for a closed kind and the TypeScript caller executes the
//! statement on its own already-open unit with the original positional binds.
//! No SQL is accepted from the wire and no TypeScript copy remains.
use erpx_server_shared::public::BusinessError;

pub const AGENT_RD_HOME_AI_NAME: &str = "agent_rd_home_ai_name";
pub const AGENT_RD_ASSISTANT_NAME_HISTORY: &str = "agent_rd_assistant_name_history";
pub const AGENT_RD_PROVIDER_SOURCE_WITNESS: &str = "agent_rd_provider_source_witness";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
    pub scope: &'static str,
}

/// Exact retired `collaborationHomeSources.aiName` literal.
const SQL_HOME_AI_NAME: &str =
    "SELECT home_ai_name FROM ai_company_settings WHERE company_id=?";

/// Exact retired `collaborationTextSources.previousAssistantName` literal.
const SQL_ASSISTANT_NAME_HISTORY: &str =
    "SELECT 1 FROM ai_home_name_history WHERE company_id=? AND name=?";

/// Exact retired `socialAiProviderSourceWitness` literal. The three nested
/// settings/connection/revision snapshots are owner-owned; only the five
/// caller binds vary.
const SQL_PROVIDER_SOURCE_WITNESS: &str = "SELECT jsonb_build_object('settings',(SELECT to_jsonb(s) FROM ai_company_settings s WHERE s.company_id=?),'connection',(SELECT to_jsonb(c) FROM ai_connections c WHERE c.company_id=? AND c.id=?),'revision',(SELECT to_jsonb(r) FROM ai_connection_revisions r WHERE r.connection_id=? AND r.revision=?))::text image";

const READS: &[NativeReadStatement] = &[
    NativeReadStatement {
        kind: AGENT_RD_HOME_AI_NAME,
        sql: SQL_HOME_AI_NAME,
        bind_count: 1,
        scope: "holding",
    },
    NativeReadStatement {
        kind: AGENT_RD_ASSISTANT_NAME_HISTORY,
        sql: SQL_ASSISTANT_NAME_HISTORY,
        bind_count: 2,
        scope: "holding",
    },
    NativeReadStatement {
        kind: AGENT_RD_PROVIDER_SOURCE_WITNESS,
        sql: SQL_PROVIDER_SOURCE_WITNESS,
        bind_count: 5,
        scope: "holding",
    },
];

pub fn read_statement(kind: &str) -> Option<&'static NativeReadStatement> {
    READS.iter().find(|statement| statement.kind == kind)
}

pub fn is_owner_read(kind: &str) -> bool {
    read_statement(kind).is_some()
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "AGENT_OWNER_READ_INVALID",
        "Review the Agent owner read.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_literals_are_owner_owned_byte_for_byte() {
        let expected: &[(&str, &str, usize)] = &[
            (
                AGENT_RD_HOME_AI_NAME,
                "SELECT home_ai_name FROM ai_company_settings WHERE company_id=?",
                1,
            ),
            (
                AGENT_RD_ASSISTANT_NAME_HISTORY,
                "SELECT 1 FROM ai_home_name_history WHERE company_id=? AND name=?",
                2,
            ),
            (
                AGENT_RD_PROVIDER_SOURCE_WITNESS,
                "SELECT jsonb_build_object('settings',(SELECT to_jsonb(s) FROM ai_company_settings s WHERE s.company_id=?),'connection',(SELECT to_jsonb(c) FROM ai_connections c WHERE c.company_id=? AND c.id=?),'revision',(SELECT to_jsonb(r) FROM ai_connection_revisions r WHERE r.connection_id=? AND r.revision=?))::text image",
                5,
            ),
        ];
        assert_eq!(expected.len(), READS.len());
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_owner_read(kind), "{kind}");
        }
        assert_eq!(read_statement("agent_rd_unknown"), None);
        assert!(!is_owner_read("agent.owner-write.v1"));
    }

    #[test]
    fn reads_are_static_select_only_and_lock_free() {
        for read in READS {
            let sql = read.sql.trim_start();
            assert!(sql.starts_with("SELECT"), "{}", read.kind);
            assert!(!sql.contains("FOR UPDATE"), "{}", read.kind);
            assert!(!sql.contains("INSERT"), "{}", read.kind);
            assert!(!sql.contains("UPDATE"), "{}", read.kind);
            assert!(!sql.contains("DELETE"), "{}", read.kind);
            assert!(!sql.contains('$'), "{}", read.kind);
        }
    }
}
