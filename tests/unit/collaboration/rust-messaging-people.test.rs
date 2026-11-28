//! Owner application/application-port checks for the native Messaging people
//! picker. The retained TypeScript authority returned `{id,name,email}` rows
//! ordered by name, filtered on `${name} ${email}` case-insensitively and
//! capped at 100; these tests pin that contract on the owner side.
//!
//! Registration note for the lead: add the matching `[[test]]` stanza to
//! `modules/collaboration/server/Cargo.toml` (manifest edits are lead-owned).

use erpx_collaboration_server::public::messaging_people::{
    project, MessagingPeopleFuture, MessagingPeopleRead, PeopleQuery, PeopleService, PersonRow,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

#[derive(Default)]
struct FakeRead {
    rows: Vec<PersonRow>,
}

impl MessagingPeopleRead for FakeRead {
    fn active<'a>(
        &'a self,
        company_id: &'a str,
        now: i64,
        viewer_id: &'a str,
        team_id: Option<&'a str>,
    ) -> MessagingPeopleFuture<'a, Vec<PersonRow>> {
        assert_eq!(company_id, "c");
        assert_eq!(now, 1_700_000_000_000);
        assert_eq!(viewer_id, "viewer");
        assert_eq!(team_id, Some("team-1"));
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }
}

fn row(id: &str, name: &str, email: &str) -> PersonRow {
    PersonRow {
        user_id: id.into(),
        name: name.into(),
        email: email.into(),
    }
}

#[tokio::test]
async fn service_returns_the_complete_picker_payload() {
    let service = PeopleService::new(FakeRead {
        rows: vec![
            row("u2", "Bob", "bob@example.test"),
            row("u1", "Ada", "ada@example.test"),
        ],
    });
    let people = service
        .read("c", 1_700_000_000_000, "viewer", Some("ada"), Some("team-1"))
        .await
        .unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!(people[0].id, "u1");
    assert_eq!(people[0].name, "Ada");
    assert_eq!(people[0].email, "ada@example.test");
    let json = serde_json::to_value(&people).unwrap();
    assert_eq!(json[0].as_object().unwrap().len(), 3);
    assert!(json[0].get("email").unwrap().is_string());
}

#[test]
fn filters_on_the_combined_account_name_and_page() {
    let rows = (0..120)
        .map(|i| row(&format!("u{i}"), &format!("Name {i}"), "example.test"))
        .collect();
    let all = project(rows, &PeopleQuery::new(None, None).unwrap());
    assert_eq!(all.len(), 100);
    assert_eq!(all[0].id, "u0");

    let rows = vec![row("u1", "Ada", "ada@example.test")];
    let by_email = project(rows, &PeopleQuery::new(Some("example.test"), None).unwrap());
    assert_eq!(by_email.len(), 1);
    assert!(PeopleQuery::new(Some(&"x".repeat(161)), None).is_err());
    assert!(PeopleQuery::new(None, Some("bad team")).is_err());
}

#[tokio::test]
async fn reader_failure_fails_closed() {
    struct Failing;
    impl MessagingPeopleRead for Failing {
        fn active<'a>(
            &'a self,
            _company_id: &'a str,
            _now: i64,
            _viewer_id: &'a str,
            _team_id: Option<&'a str>,
        ) -> MessagingPeopleFuture<'a, Vec<PersonRow>> {
            Box::pin(async {
                Err(BusinessError::new(FailureKind::Unavailable, "DOWN", "down"))
            })
        }
    }
    let result = PeopleService::new(Failing)
        .read("c", 1_700_000_000_000, "viewer", None, None)
        .await;
    assert!(result.is_err());
}
