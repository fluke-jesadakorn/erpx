//! Pure legacy Collaboration workspace facts: visible-source resolution, the
//! additive presentation hook and the retained source list. Database scope
//! filtering lives in the PostgreSQL companion test.
use erpx_server_collaboration::public::collaboration_work::{
    CollaborationSourcePort, SourcePresentation, WORKSPACE_SOURCE_TYPES, hidden_source,
    resolve_source,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::record_source_contract::{
    SourceKind, SourceRecord, SourceReference,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

fn record(kind: SourceKind, id: &str, company: &str, title: &str) -> SourceRecord {
    SourceRecord {
        kind,
        id: id.into(),
        company_id: company.into(),
        branch_id: Some("branch".into()),
        department_id: None,
        version: 3,
        title: title.into(),
        status: "open".into(),
    }
}

#[derive(Default)]
struct Port {
    prepared: usize,
    witnessed: usize,
}

impl CollaborationSourcePort for Port {
    type Error = BusinessError;
    type Witness = String;

    async fn prepare(
        &mut self,
        reference: &SourceReference,
    ) -> Result<(SourceRecord, Self::Witness), BusinessError> {
        self.prepared += 1;
        Ok((
            record(reference.kind, &reference.id, "company", "Prepared title"),
            format!("witness:{}", reference.id),
        ))
    }

    async fn require_current(&mut self, witness: &Self::Witness) -> Result<(), BusinessError> {
        self.witnessed += 1;
        assert!(witness.starts_with("witness:"));
        Ok(())
    }
}

#[tokio::test]
async fn source_port_presentation_is_additive_and_keeps_the_witness_contract() {
    let mut port = Port::default();
    let reference = SourceReference {
        kind: SourceKind::SupportTicket,
        id: "ticket-1".into(),
        expected_version: None,
    };
    // Existing implementors that only supply prepare/require_current keep
    // compiling and report no invented destination.
    assert_eq!(
        port.present(&reference).await.unwrap(),
        SourcePresentation::default()
    );
    let (source, witness) = port.prepare(&reference).await.unwrap();
    assert_eq!(source.title, "Prepared title");
    assert_eq!(witness, "witness:ticket-1");
    port.require_current(&witness).await.unwrap();
    assert_eq!((port.prepared, port.witnessed), (1, 1));
}

#[test]
fn resolved_source_requires_matching_identity_and_company() {
    let prepared = record(SourceKind::SupportTicket, "ticket-1", "company", "Ticket");
    let presentation = SourcePresentation {
        title: None,
        href: Some("/support/tickets/ticket-1".into()),
    };
    let visible = resolve_source(
        "company",
        SourceKind::SupportTicket,
        "ticket-1",
        prepared.clone(),
        presentation.clone(),
    )
    .expect("matching source");
    assert_eq!(visible.id, "ticket-1");
    assert_eq!(visible.kind, SourceKind::SupportTicket);
    assert_eq!(visible.title, "Ticket");
    assert_eq!(visible.href.as_deref(), Some("/support/tickets/ticket-1"));
    assert_eq!(visible.branch_id.as_deref(), Some("branch"));
    assert_eq!(visible.department_id, None);

    // Owner-supplied title wins; an absent destination stays absent.
    let renamed = resolve_source(
        "company",
        SourceKind::SupportTicket,
        "ticket-1",
        prepared.clone(),
        SourcePresentation {
            title: Some("Owner label".into()),
            href: None,
        },
    )
    .expect("matching source");
    assert_eq!(renamed.title, "Owner label");
    assert_eq!(renamed.href, None);

    for (company, kind, id) in [
        ("other-company", SourceKind::SupportTicket, "ticket-1"),
        ("company", SourceKind::FieldServiceOrder, "ticket-1"),
        ("company", SourceKind::SupportTicket, "ticket-2"),
    ] {
        assert!(
            resolve_source(company, kind, id, prepared.clone(), presentation.clone()).is_none(),
            "accepted foreign or mismatched source {company}/{kind:?}/{id}"
        );
    }
}

#[test]
fn workspace_source_types_retain_the_non_field_owner_list() {
    assert_eq!(WORKSPACE_SOURCE_TYPES.len(), 8);
    assert_eq!(WORKSPACE_SOURCE_TYPES[0], "commerce_order");
    assert_eq!(WORKSPACE_SOURCE_TYPES[1], "support_ticket");
    assert_eq!(WORKSPACE_SOURCE_TYPES[7], "maintenance_work");
    assert!(!WORKSPACE_SOURCE_TYPES.contains(&"field_service_order"));
    for kind in WORKSPACE_SOURCE_TYPES {
        assert!(
            SourceKind::parse(kind).is_ok(),
            "workspace lists an unknown source kind {kind}"
        );
    }
}

#[test]
fn only_source_owner_forbidden_and_not_found_errors_hide_a_row() {
    let business = |kind| PgTransactionError::Business(BusinessError::new(kind, "X", "x"));
    assert!(hidden_source(&business(FailureKind::Forbidden)));
    assert!(hidden_source(&business(FailureKind::NotFound)));
    for kind in [
        FailureKind::InvalidInput,
        FailureKind::Unauthenticated,
        FailureKind::Conflict,
        FailureKind::Unavailable,
    ] {
        assert!(
            !hidden_source(&business(kind)),
            "hid a row for failure {kind:?}"
        );
    }
    let persistence: PgTransactionError<BusinessError> =
        PersistenceError::new("COLLABORATION_TEST_PERSISTENCE").into();
    assert!(!hidden_source(&persistence));
}
