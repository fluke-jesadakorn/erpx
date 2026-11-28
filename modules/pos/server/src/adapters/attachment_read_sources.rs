//! Native POS attachment source read kinds. These closed, `SELECT`-only owner
//! statements are the runtime authority for the retained attachment source
//! image read (`attachment-read-sources.ts`). The caller supplies only the
//! pinned source kind, its id and the caller's Company value; the table is
//! selected here from the allow-listed kind, never from caller text. There is
//! no TypeScript SQL, mirror or fallback for this family. The integration
//! oracle imports these statements so the runtime registry and the parity test
//! cannot drift apart.

pub const POS_SALE: &str = "pos_attachment_source_pos_sale";
pub const POS_REFUND: &str = "pos_attachment_source_pos_refund";
pub const POS_SHIFT: &str = "pos_attachment_source_pos_shift";

pub const KINDS: &[&str] = &[POS_SALE, POS_REFUND, POS_SHIFT];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the three attachment source kinds, byte-identical to
/// the retired `attachment-read-sources.ts` literals. Every statement is a
/// caller-transaction `SELECT` scoped by the caller-supplied id and Company.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        POS_SALE => "SELECT to_jsonb(source)::text image FROM pos_sales source WHERE source.id=? AND source.company_id=?",
        POS_REFUND => "SELECT to_jsonb(source)::text image FROM pos_refunds source WHERE source.id=? AND source.company_id=?",
        POS_SHIFT => "SELECT to_jsonb(source)::text image FROM pos_shifts source WHERE source.id=? AND source.company_id=?",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_source_kinds_are_select_only_and_parameterized() {
        assert_eq!(KINDS.len(), 3);
        for kind in KINDS {
            assert!(supported(kind), "kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
            assert_eq!(stmt.matches('?').count(), 2, "kind must carry id/Company scope: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "${"] {
                assert!(!stmt.contains(forbidden), "kind must not mutate or interpolate: {kind} contains {forbidden}");
            }
        }
    }

    #[test]
    fn unknown_attachment_source_kind_fails_closed() {
        assert!(!supported("pos_attachment_source_unlisted"));
        assert!(sql("pos_attachment_source_unlisted").is_none());
    }
}
