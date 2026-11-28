//! Files' registered Count action evidence contexts, projected for Records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CountEvidencePolicy {
    pub target_type: &'static str,
    pub purpose_key: &'static str,
    pub component_kind: &'static str,
}
const POLICIES: [CountEvidencePolicy; 4] = [
    CountEvidencePolicy {
        target_type: "stock_count",
        purpose_key: "count_sheet",
        component_kind: "action",
    },
    CountEvidencePolicy {
        target_type: "stock_count",
        purpose_key: "count_evidence",
        component_kind: "action",
    },
    CountEvidencePolicy {
        target_type: "stock_count",
        purpose_key: "count_photo",
        component_kind: "action",
    },
    CountEvidencePolicy {
        target_type: "inventory_stock_count",
        purpose_key: "count_evidence",
        component_kind: "action",
    },
];
pub fn count_evidence_policies() -> &'static [CountEvidencePolicy] {
    &POLICIES
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn count_action_contexts_match_retained_registry() {
        let actual = count_evidence_policies()
            .iter()
            .map(|p| (p.target_type, p.purpose_key, p.component_kind))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                ("stock_count", "count_sheet", "action"),
                ("stock_count", "count_evidence", "action"),
                ("stock_count", "count_photo", "action"),
                ("inventory_stock_count", "count_evidence", "action")
            ]
        );
    }
}
