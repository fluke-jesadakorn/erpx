use erpx_server_collaboration::public::collaboration_work::{
    AuthorizedActor, CardAction, Change, CollaborationUnit, CurrentCard, CurrentRelation,
    OwnerPlan, Receipt, RelationKind, execute,
};
use erpx_server_shared::public::record_source_contract::{
    SourceKind, SourceRecord, SourceReference,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

struct Unit {
    steps: Vec<&'static str>,
    drift: bool,
    legacy: bool,
    replay: bool,
    actor: AuthorizedActor,
}
impl erpx_server_collaboration::public::collaboration_work::CollaborationSourcePort for Unit {
    type Error = BusinessError;
    type Witness = String;
    async fn prepare(
        &mut self,
        r: &SourceReference,
    ) -> Result<(SourceRecord, String), BusinessError> {
        self.steps.push("source");
        Ok((
            SourceRecord {
                kind: r.kind,
                id: r.id.clone(),
                company_id: "company".into(),
                branch_id: None,
                department_id: None,
                version: 3,
                title: "Source".into(),
                status: "open".into(),
            },
            r.id.clone(),
        ))
    }
    async fn require_current(&mut self, _: &String) -> Result<(), BusinessError> {
        self.steps.push("current");
        if self.drift {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SOURCE_RECORD_CHANGED",
                "changed",
            ));
        }
        Ok(())
    }
}
impl CollaborationUnit for Unit {
    async fn authorize(&mut self) -> Result<AuthorizedActor, BusinessError> {
        self.steps.push("authorize");
        Ok(self.actor.clone())
    }
    async fn replay(&mut self, _: &Change) -> Result<Option<Receipt>, BusinessError> {
        self.steps.push("replay");
        Ok(self.replay.then(|| Receipt {
            id: "retained".into(),
            version: 1,
            status: "active",
        }))
    }
    async fn relation(&mut self, id: &str) -> Result<CurrentRelation, BusinessError> {
        let mut left = reference("left");
        let mut right = reference("right");
        if self.legacy {
            left.kind = SourceKind::SupportTicket;
            right.kind = SourceKind::SupportTicket;
        }
        Ok(CurrentRelation {
            id: id.into(),
            version: 1,
            active: true,
            left,
            right,
        })
    }
    async fn card(&mut self, id: &str) -> Result<CurrentCard, BusinessError> {
        Ok(CurrentCard {
            id: id.into(),
            version: 1,
            open: true,
            owner_id: "someone-else".into(),
            due_at: 1234,
            owner_eligible: true,
            source: {
                let mut r = reference("field");
                if self.legacy {
                    r.kind = SourceKind::SupportTicket;
                }
                r
            },
        })
    }
    async fn apply(&mut self, _: &AuthorizedActor, _: OwnerPlan) -> Result<Receipt, BusinessError> {
        self.steps.push("apply");
        Ok(Receipt {
            id: "new".into(),
            version: 1,
            status: "active",
        })
    }
    async fn flush_deferred(&mut self) -> Result<(), BusinessError> {
        self.steps.push("flush");
        Ok(())
    }
    async fn require_unit_current(&mut self) -> Result<(), BusinessError> {
        self.steps.push("authority");
        Ok(())
    }
}
fn reference(id: &str) -> SourceReference {
    SourceReference {
        kind: SourceKind::FieldServiceOrder,
        id: id.into(),
        expected_version: None,
    }
}
fn unit() -> Unit {
    Unit {
        steps: vec![],
        drift: false,
        legacy: false,
        replay: false,
        actor: AuthorizedActor {
            user_id: "actor".into(),
            company_id: "company".into(),
            can_review: false,
        },
    }
}
#[tokio::test]
async fn relation_retains_both_owner_sources_after_deferred_effects() {
    let mut u = unit();
    let result = execute(
        &mut u,
        Change::CreateRelation {
            left: reference("one"),
            right: reference("two"),
            relation: RelationKind::Related,
            reason: "reviewed".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(result.id, "new");
    assert_eq!(
        u.steps,
        [
            "authorize",
            "replay",
            "source",
            "source",
            "apply",
            "flush",
            "current",
            "current",
            "authority"
        ]
    );
}
#[tokio::test]
async fn replay_still_checks_current_sources_and_selected_authority() {
    let mut u = unit();
    u.replay = true;
    u.drift = true;
    let e = execute(
        &mut u,
        Change::CreateRelation {
            left: reference("one"),
            right: reference("two"),
            relation: RelationKind::Related,
            reason: "reviewed".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(e.code, "SOURCE_RECORD_CHANGED");
    assert!(!u.steps.contains(&"apply"));
    assert!(u.steps.contains(&"flush"));
}
#[tokio::test]
async fn completion_requires_the_actual_assigned_owner() {
    let mut u = unit();
    let e = execute(
        &mut u,
        Change::ChangeCard {
            id: "card".into(),
            expected_version: 1,
            action: CardAction::Complete,
            owner_id: None,
            due_at: None,
            evidence: Some("done".into()),
            reason: "reviewed".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(e.code, "ACTION_CARD_OWNER_REQUIRED");
    assert!(!u.steps.contains(&"apply"));
}
#[tokio::test]
async fn native_partition_rejects_unported_relation_before_replay_or_owner_effects() {
    let mut u = unit();
    let mut other = reference("ticket");
    other.kind = SourceKind::CommerceOrder;
    let error = execute(
        &mut u,
        Change::CreateRelation {
            left: reference("field"),
            right: other,
            relation: RelationKind::Related,
            reason: "reviewed".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "COLLABORATION_NATIVE_SOURCE_UNSUPPORTED");
    assert_eq!(u.steps, ["authorize"]);
}
#[tokio::test]
async fn reason_uses_ecmascript_trim_and_utf16_length() {
    let mut u = unit();
    let error = execute(
        &mut u,
        Change::CreateRelation {
            left: reference("one"),
            right: reference("two"),
            relation: RelationKind::Related,
            reason: "\u{feff}ab\u{feff}".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "COLLABORATION_INPUT_INVALID");
    assert_eq!(u.steps, ["authorize"]);
}

#[test]
fn writer_partition_uses_both_stored_relation_endpoints() {
    use erpx_server_collaboration::public::collaboration_work::{
        WriterPartition, card_partition, relation_partition,
    };
    assert_eq!(
        relation_partition(SourceKind::FieldServiceOrder, SourceKind::FieldServiceOrder),
        WriterPartition::NativeField
    );
    assert_eq!(
        relation_partition(SourceKind::FieldServiceOrder, SourceKind::SupportTicket),
        WriterPartition::NativeField
    );
    assert_eq!(
        relation_partition(SourceKind::SupportTicket, SourceKind::FieldServiceOrder),
        WriterPartition::NativeField
    );
    assert_eq!(
        card_partition(SourceKind::FieldServiceOrder),
        WriterPartition::NativeField
    );
    assert_eq!(
        card_partition(SourceKind::SupportTicket),
        WriterPartition::Legacy
    );
}

#[test]
fn wire_normalization_keeps_only_schema_fields_and_preserves_removal_receipts() {
    use erpx_server_collaboration::public::collaboration_work::decode_work_command;
    use serde_json::json;
    let create = decode_work_command("collaboration.action.create", None, &json!({
        "sourceType":"field_service_order", "sourceId":"original-id", "title":"\u{feff} Review 😀 \u{feff}",
        "ownerId":"user", "dueAt":1234.0, "reason":"  reviewed  ", "ignored":"discard", "action":"cancel"
    })).unwrap();
    assert_eq!(
        create.input,
        json!({"sourceType":"field_service_order","sourceId":"original-id","title":"Review 😀","ownerId":"user","dueAt":1234,"reason":"reviewed"})
    );
    let remove = decode_work_command(
        "collaboration.relation.remove",
        Some("relation"),
        &json!({"expectedVersion":1,"reason":"  reviewed  ","ignored":true}),
    )
    .unwrap();
    assert_eq!(
        remove.input,
        json!({"expectedVersion":1,"reason":"  reviewed  "})
    );
    assert!(
        matches!(remove.change, Change::RemoveRelation { reason, .. } if reason == "  reviewed  ")
    );
}

#[test]
fn wire_rejects_route_action_mismatch_and_invalid_javascript_integer_values() {
    use erpx_server_collaboration::public::collaboration_work::decode_work_command;
    use serde_json::json;
    assert!(
        decode_work_command(
            "collaboration.action.cancel",
            Some("card"),
            &json!({"action":"complete","expectedVersion":1,"evidence":"done","reason":"reviewed"})
        )
        .is_err()
    );
    for invalid in [
        json!(0),
        json!(-1),
        json!(1.5),
        json!("1"),
        json!(null),
        json!(9007199254740992_u64),
    ] {
        let result = decode_work_command(
            "collaboration.action.reassign",
            Some("card"),
            &json!({"action":"reassign","expectedVersion":1,"ownerId":"user","dueAt":invalid,"reason":"reviewed"}),
        );
        assert!(result.is_err(), "accepted invalid dueAt {invalid}");
    }
    assert!(
        decode_work_command(
            "collaboration.action.cancel",
            None,
            &json!({"action":"cancel","expectedVersion":1,"reason":"reviewed"})
        )
        .is_err()
    );
}

#[test]
fn wire_keeps_asymmetric_relations_and_ignores_irrelevant_transition_fields() {
    use erpx_server_collaboration::public::collaboration_work::decode_work_command;
    use serde_json::json;
    let relation = decode_work_command("collaboration.relation.create", None, &json!({"sourceType":"field_service_order","sourceId":"z","targetType":"field_service_order","targetId":"a","relation":"depends_on","reason":"reviewed"})).unwrap();
    assert!(
        matches!(relation.change, Change::CreateRelation { left, right, relation: RelationKind::DependsOn, .. } if left.id == "z" && right.id == "a")
    );
    let cancel = decode_work_command("collaboration.action.cancel", Some("card"), &json!({"action":"cancel","expectedVersion":1,"ownerId":"malicious-other-owner","dueAt":1,"evidence":"ignored","reason":"reviewed"})).unwrap();
    assert_eq!(
        cancel.input,
        json!({"action":"cancel","expectedVersion":1,"reason":"reviewed"})
    );
    assert!(matches!(
        cancel.change,
        Change::ChangeCard {
            owner_id: None,
            due_at: None,
            evidence: None,
            ..
        }
    ));
}

#[test]
fn native_action_work_retains_ineligible_owner_and_terminal_behavior() {
    use erpx_server_collaboration::public::collaboration_work::{ActionWorkInput, action_work};
    let mut r = ActionWorkInput {
        id: "card".into(),
        title: "Review".into(),
        source_type: "field_service_order".into(),
        source_id: "order".into(),
        status: "open".into(),
        owner_id: "owner".into(),
        created_by: "maker".into(),
        owner_eligible: 1,
        due_at: 10,
        updated_at: 1,
    };
    let own = action_work(&r, "owner", false);
    assert!(own.can_act);
    assert_eq!(own.relationships, vec!["assigned"]);
    r.owner_eligible = 0;
    let ineligible = action_work(&r, "owner", false);
    assert!(!ineligible.can_act);
    assert_eq!(ineligible.queue, "waiting");
    assert!(ineligible.blocker.is_some());
    assert!(action_work(&r, "reviewer", true).can_act);
    for status in ["completed", "cancelled"] {
        r.status = status.into();
        let w = action_work(&r, "reviewer", true);
        assert_eq!(w.queue, "completed");
        assert!(!w.can_act);
    }
}

#[tokio::test]
async fn field_support_relations_retain_both_actual_source_guards() {
    for reverse in [false, true] {
        let mut u = unit();
        let mut support = reference("ticket");
        support.kind = SourceKind::SupportTicket;
        let field = reference("field");
        let (left, right) = if reverse {
            (support, field)
        } else {
            (field, support)
        };
        execute(
            &mut u,
            Change::CreateRelation {
                left,
                right,
                relation: RelationKind::Blocks,
                reason: "Actual cross-owner handoff".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(u.steps.iter().filter(|s| **s == "source").count(), 2);
        assert_eq!(u.steps.iter().filter(|s| **s == "current").count(), 2);
        assert!(u.steps.contains(&"flush"));
    }
}

#[tokio::test]
async fn stored_legacy_partition_never_returns_a_native_replay_or_stages_effects() {
    for replay in [false, true] {
        for relation in [false, true] {
            let mut u = unit();
            u.legacy = true;
            u.replay = replay;
            u.actor.can_review = true;
            let change = if relation {
                Change::RemoveRelation {
                    id: "legacy".into(),
                    expected_version: 1,
                    reason: "Reviewed removal".into(),
                }
            } else {
                Change::ChangeCard {
                    id: "legacy".into(),
                    expected_version: 1,
                    action: CardAction::Cancel,
                    owner_id: None,
                    due_at: None,
                    evidence: None,
                    reason: "Reviewed cancellation".into(),
                }
            };
            assert_eq!(
                execute(&mut u, change).await.unwrap_err().code,
                "COLLABORATION_NATIVE_SOURCE_UNSUPPORTED"
            );
            assert!(!u.steps.contains(&"source"));
            assert!(!u.steps.contains(&"apply"));
            assert!(!u.steps.contains(&"flush"));
        }
    }
}

#[test]
fn native_push_policy_preserves_quiet_hours_categories_and_recovery() {
    use erpx_server_collaboration::public::web_push::*;
    let mut p = PushPreferences {
        enabled: true,
        work: true,
        mention: false,
        system: true,
        calendar: false,
        notice: false,
        quiet_start: Some(22 * 60),
        quiet_end: Some(7 * 60),
    };
    assert!(p.permits("work"));
    assert!(!p.permits("mention"));
    assert!(!p.permits("unregistered"));
    assert_eq!(p.quiet_delay(15 * 60 * 60 * 1000), 9 * 60 * 60 * 1000);
    assert_eq!(p.quiet_delay(1234), 0);
    assert_eq!(
        p.quiet_delay(15 * 60 * 60 * 1000 + 1234),
        9 * 60 * 60 * 1000 - 1234
    );
    p.quiet_start = Some(9 * 60);
    p.quiet_end = Some(17 * 60);
    assert_eq!(p.quiet_delay(2 * 60 * 60 * 1000), 8 * 60 * 60 * 1000);
    p.enabled = false;
    assert!(!p.permits("work"));
    assert_eq!(retry(1, Some(410), 100), PushRetry::RemoveSubscription);
    assert_eq!(retry(1, Some(404), 100), PushRetry::RemoveSubscription);
    assert_eq!(retry(1, Some(403), 100), PushRetry::Dead);
    assert_eq!(retry(5, None, 100), PushRetry::Dead);
    assert_eq!(retry(2, Some(429), 100), PushRetry::RetryAt(4100));
    assert_eq!(retry(1, None, 100), PushRetry::RetryAt(2100));
}
#[cfg(feature = "push-provider")]
mod push_provider_conformance {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
    use erpx_server_collaboration::public::web_push::*;
    const PUBLIC: &str =
        "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
    const PRIVATE: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";
    const RECEIVER: &str =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    #[test]
    fn rfc8291_appendix_a_ciphertext_and_header_match() {
        let decode = |s: &str| B64.decode(s).unwrap();
        let encrypted = encrypt_message(
            &decode("V2hlbiBJIGdyb3cgdXAsIEkgd2FudCB0byBiZSBhIHdhdGVybWVsb24"),
            &decode(RECEIVER),
            &decode("BTBZMqHH6r4Tts7J_aSIgg"),
            &decode(PUBLIC),
            &decode("kyrL1jIIOHEzg3sM2ZWRHDRB62YACZhhSlknJ672kSs"),
            &decode("DGv6ra1nlYgDCS1FRnbzlw"),
        )
        .unwrap();
        assert_eq!(
            B64.encode(&encrypted[..86]),
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8"
        );
        assert_eq!(
            B64.encode(&encrypted[86..]),
            "8pfeW0KbunFT06SuDKoJH9Ql87S1QUrdirN6GcG7sFz1y1sqLgVi1VhjVkHsUoEsbI_0LpXMuGvnzQ"
        );
    }
    #[test]
    fn node_sealed_envelope_vapid_signature_and_fresh_ephemeral_entropy() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/collaboration/web-push/native-provider.v1.json"
        ))
        .unwrap();
        let provider = NativePushProvider::new(
            PUBLIC,
            PRIVATE,
            "mailto:synthetic@example.test",
            fixture["dataKey"].as_str().unwrap(),
        )
        .unwrap();
        let sealed = fixture["sealed"].as_str().unwrap();
        let request = provider
            .prepare(
                sealed,
                br#"{"tag":"job","path":"/inbox"}"#,
                1_700_000_000_000,
            )
            .unwrap();
        let (url, authorization, body) = request.parts();
        assert_eq!(url.host_str(), Some("fcm.googleapis.com"));
        assert_ne!(
            body,
            provider
                .prepare(sealed, b"payload", 1_700_000_000_000)
                .unwrap()
                .parts()
                .2
        );
        let token = authorization
            .strip_prefix("vapid t=")
            .unwrap()
            .split(", k=")
            .next()
            .unwrap();
        let parts: Vec<_> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: serde_json::Value =
            serde_json::from_slice(&B64.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["exp"], 1_700_043_200i64);
        assert_eq!(claims["sub"], "mailto:synthetic@example.test");
        let raw = B64.decode(parts[2]).unwrap();
        assert_eq!(raw.len(), 64);
        ring::signature::UnparsedPublicKey::new(
            &ring::signature::ECDSA_P256_SHA256_FIXED,
            B64.decode(PUBLIC).unwrap(),
        )
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &raw)
        .unwrap();
        assert!(provider.prepare("broken", b"payload", 1).is_err());
        assert!(provider.prepare(sealed, &vec![0; 3994], 1).is_err());
    }
    #[test]
    fn endpoint_curve_and_vapid_configuration_reject_untrusted_values() {
        for endpoint in [
            "http://fcm.googleapis.com/test",
            "https://fcm.googleapis.com:443/test",
            "https://fcm.googleapis.com.evil.test/",
            "https://127.0.0.1/",
            "https://user@fcm.googleapis.com/",
            "https://fcm.googleapis.com/test#private",
        ] {
            let subscription = PushSubscription {
                endpoint: endpoint.into(),
                keys: PushSubscriptionKeys {
                    p256dh: RECEIVER.into(),
                    auth: "BTBZMqHH6r4Tts7J_aSIgg".into(),
                },
            };
            assert!(subscription.validate().is_err());
        }
        let subscription = PushSubscription {
            endpoint: "https://fcm.googleapis.com/test".into(),
            keys: PushSubscriptionKeys {
                p256dh: B64.encode([0u8; 65]),
                auth: "BTBZMqHH6r4Tts7J_aSIgg".into(),
            },
        };
        assert!(subscription.validate().is_err());
        assert!(
            NativePushProvider::new(
                PUBLIC,
                &B64.encode([0u8; 32]),
                "mailto:synthetic@example.test",
                &B64.encode([0u8; 32])
            )
            .is_err()
        );
    }
}
