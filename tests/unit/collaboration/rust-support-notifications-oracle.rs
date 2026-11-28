// Native-seeded oracle for the eleven closed Support notification effects.
// Replaces the retired TypeScript writer-oracle assertions with independent,
// native-derived recipient/message/id/error proofs over every effect. This
// file is included by `support_notification.rs`'s `#[cfg(test)]` module, so it
// runs with `cargo test -p erpx-collaboration-server --lib support_notification`.
// It is deliberately not named `*.test.rs`: Cargo `[[test]]` targets and the
// catalogue suite manifest are outside this migration slice's allowed paths.
// Items are in scope through the including module's `use super::*`.

fn facts() -> SupportNotificationFacts {
    SupportNotificationFacts {
        command_key: "support-command-01".into(),
        company_id: "company-1".into(),
        record_id: "support-ticket-77".into(),
        version: 3,
        updated_at: 1_800_000_000_123,
    }
}

#[test]
fn support_notification_oracle_all_eleven_effects_derive_native_rows() {
    let recipient = "manager-1";
    let expected = [
        (
            SupportNotificationEffect::IncidentUpdated,
            "support_incident",
            "Incident updated: review impact and follow up each linked customer case",
        ),
        (
            SupportNotificationEffect::TicketReopened,
            "support_ticket",
            "New issue needs Support triage",
        ),
        (
            SupportNotificationEffect::TicketCreated,
            "support_ticket",
            "New customer support case: accept and review the issue",
        ),
        (
            SupportNotificationEffect::TicketUpdated,
            "support_ticket",
            "Support case updated: review current status and next action",
        ),
        (
            SupportNotificationEffect::TicketReviewers,
            "support_ticket",
            "Review proposed issue resolution",
        ),
        (
            SupportNotificationEffect::SlaBreached,
            "support_sla_alert",
            "Resolution target breached: take responsibility for follow-up",
        ),
        (
            SupportNotificationEffect::SessionReviewers,
            "support_session",
            "Review a temporary customer support session",
        ),
        (
            SupportNotificationEffect::SessionUpdated,
            "support_session",
            "Temporary support access changed: review current grants",
        ),
        (
            SupportNotificationEffect::PolicyReviewers,
            "support_policy",
            "Review a proposed working-time resolution policy",
        ),
        (
            SupportNotificationEffect::PolicyUpdated,
            "support_policy",
            "SLA policy reviewed: check its current availability",
        ),
        (
            SupportNotificationEffect::TicketClockAttached,
            "support_ticket",
            "Approved working-time target attached: review the deadline",
        ),
    ];
    assert_eq!(expected.len(), 11);
    for (effect, record_type, message) in expected {
        let f = facts();
        let row = effect.derive(&f, recipient).unwrap();
        assert_eq!(row.record_type, record_type);
        assert_eq!(row.message, message);
        assert_eq!(row.user_id, recipient);
        assert_eq!(row.company_id, f.company_id);
        assert_eq!(row.record_id, f.record_id);
        assert_eq!(row.record_version, f.version);
        assert_eq!(row.created_at, f.updated_at);
        assert_eq!(row.read_at, None);
        assert_eq!(row.conflict_safe(), effect.conflict_safe());
        assert_eq!(row.event_id(), format!("legacy:{}", row.id));
        assert_eq!(row.image()["id"], serde_json::json!(row.id));

        let expected_id = match effect {
            SupportNotificationEffect::SlaBreached => format!("{}:{}", f.record_id, recipient),
            e if e.multi_recipient() => format!("{}{}", f.command_key, recipient),
            _ => f.command_key.clone(),
        };
        assert_eq!(row.id, expected_id);
    }
}

#[test]
fn support_notification_oracle_rejects_empty_and_invalid_facts() {
    for field in ["command_key", "company_id", "record_id"] {
        let mut f = facts();
        match field {
            "command_key" => f.command_key.clear(),
            "company_id" => f.company_id.clear(),
            "record_id" => f.record_id.clear(),
            _ => unreachable!(),
        }
        assert!(SupportNotificationEffect::TicketCreated.derive(&f, "u").is_err());
    }
    let mut f = facts();
    f.version = 0;
    assert!(SupportNotificationEffect::TicketCreated.derive(&f, "u").is_err());
    let f = facts();
    assert!(SupportNotificationEffect::TicketCreated.derive(&f, "").is_err());
}

#[test]
fn support_notification_oracle_round_trips_every_effect_name() {
    for effect in SupportNotificationEffect::ALL {
        assert_eq!(
            SupportNotificationEffect::from_str(effect.as_str()).unwrap(),
            effect
        );
    }
    assert!(SupportNotificationEffect::from_str("nope").is_err());
}
