//! Production Position GPO Directory connector rules: approved URL/TLS gate,
//! secret-profile scope, runner/UDM inventory, action payload and read-back.
//! A bounded in-memory transport stands in for the installed connector; every
//! production HTTP request would use the approved HTTPS transport.
#![allow(dead_code)]
use erpx_server_identity_access::public::position_gpo::{
    ConnectionRow, ConnectorReply, ConnectorRequest, ConnectorTransport, GpoAction, Inventory,
    InventoryObject, PositionGpoConnector, PositionGpoDirectory,
};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

type SendFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

struct Fake {
    requests: Arc<Mutex<Vec<ConnectorRequest>>>,
    reply: Box<dyn Fn(&ConnectorRequest) -> ConnectorReply + Send + Sync>,
}

impl ConnectorTransport for Fake {
    fn send<'a>(
        &'a self,
        request: ConnectorRequest,
    ) -> SendFuture<'a, Result<ConnectorReply, BusinessError>> {
        self.requests.lock().unwrap().push(request.clone());
        let reply = (self.reply)(&request);
        Box::pin(async move { Ok(reply) })
    }
}

fn reply(status: u16, body: Value) -> ConnectorReply {
    ConnectorReply {
        status,
        etag: None,
        location: None,
        retry_after: None,
        body,
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}

fn profile(extra: Value) -> Value {
    let mut base = json!({
        "allowedHosts": ["udm.example.test", "runner.example.test"],
        "connectionIds": ["connection-1"],
        "udmUrl": "https://udm.example.test/univention/udm/",
        "udmUsername": "administrator",
        "udmPassword": "directory-secret",
        "identityAliases": {"windows-guid": "account-1"},
        "writeOperations": ["gpo.filter", "gpo.link", "group.members"],
        "writeBaseDns": ["ou=Lab,dc=example,dc=test"]
    });
    if let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    json!({"lab": base})
}

fn profile_file(document: &Value) -> tempfile::NamedTempFile {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(serde_json::to_vec(document).unwrap().as_slice())
        .unwrap();
    file
}

fn connection(provider: &str) -> ConnectionRow {
    ConnectionRow {
        id: "connection-1".into(),
        provider: provider.into(),
        config: json!({
            "secretProfile": "lab",
            "baseDn": "ou=Lab,dc=example,dc=test",
            "mode": "ldaps",
            "authority": "erpx.test"
        }),
        status: "active".into(),
        version: 1,
    }
}

fn connector(
    document: &Value,
    requests: Arc<Mutex<Vec<ConnectorRequest>>>,
    respond: impl Fn(&ConnectorRequest) -> ConnectorReply + Send + Sync + 'static,
) -> PositionGpoConnector<Fake> {
    let path = profile_file(document).into_temp_path().keep().unwrap();
    PositionGpoConnector::new(
        path,
        Fake {
            requests,
            reply: Box::new(respond),
        },
    )
}

fn udm_inventory_body(path: &str) -> ConnectorReply {
    if path.ends_with("openapi.json") {
        return reply(200, json!({"info": {"version": "5.0"}}));
    }
    if path.ends_with("users/user/") {
        return reply(
            200,
            json!({
                "_embedded": {"udm:object": [
                    {"dn": "uid=ada,ou=Lab,dc=example,dc=test"},
                    {"dn": "uid=outside,ou=Other,dc=example,dc=test"}
                ]}
            }),
        );
    }
    if path.ends_with("groups/group/") {
        return reply(
            200,
            json!({"_embedded": {"udm:object": [{"dn": "cn=Team,ou=Lab,dc=example,dc=test"}]}}),
        );
    }
    if path.ends_with("container/ou/") || path.ends_with("container/cn/") {
        return reply(200, json!({"results": 0}));
    }
    if path.ends_with("computers/windows/") {
        return reply(404, json!({}));
    }
    if path.ends_with("users/user/add") {
        return reply(
            200,
            json!({"properties": {"firstname": "", "lastname": ""}}),
        );
    }
    if path.contains("users/user/uid=ada") || path.contains("users/user/uid%3Dada") {
        let mut object = reply(
            200,
            json!({
                "uuid": "account-1",
                "dn": "uid=ada,ou=Lab,dc=example,dc=test",
                "properties": {"username": "ada", "lastname": "Lovelace"}
            }),
        );
        object.etag = Some("\"1\"".into());
        return object;
    }
    if path.contains("groups/group/cn=Team") || path.contains("groups/group/cn%3DTeam") {
        let mut object = reply(
            200,
            json!({
                "uuid": "group-1",
                "dn": "cn=Team,ou=Lab,dc=example,dc=test",
                "properties": {"name": "Team", "users": []}
            }),
        );
        object.etag = Some("\"2\"".into());
        return object;
    }
    reply(404, json!({}))
}

#[tokio::test]
async fn profile_scope_and_approved_endpoint_rules_fail_closed() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let outside =
        json!({"other": {"allowedHosts": ["udm.example.test"], "connectionIds": ["other"]}});
    let mut connector_ = connector(&outside, requests, |_| reply(200, json!({})));
    assert_eq!(
        connector_
            .inventory(&connection("univention"))
            .await
            .unwrap_err()
            .code,
        "DIRECTORY_SECRET_UNAVAILABLE"
    );

    let requests = Arc::new(Mutex::new(Vec::new()));
    let wrong_connection = profile(json!({"connectionIds": ["another-connection"]}));
    let mut connector_ = connector(&wrong_connection, requests, |_| reply(200, json!({})));
    assert_eq!(
        connector_
            .inventory(&connection("univention"))
            .await
            .unwrap_err()
            .code,
        "DIRECTORY_PROFILE_SCOPE"
    );

    for udm in [
        "http://udm.example.test/univention/udm/",
        "https://evil.example.test/univention/udm/",
        "https://administrator:secret@udm.example.test/univention/udm/",
        "https://udm.example.test/not-udm/",
    ] {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let document = profile(json!({"udmUrl": udm}));
        let mut connector_ = connector(&document, requests, |_| reply(200, json!({})));
        let error = connector_
            .inventory(&connection("univention"))
            .await
            .unwrap_err();
        assert!(
            matches!(
                error.code,
                "DIRECTORY_ENDPOINT_FORBIDDEN" | "UDM_ENDPOINT_INVALID"
            ),
            "{udm} -> {}",
            error.code
        );
    }
}

#[tokio::test]
async fn udm_inventory_reads_approved_objects_only() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut connector_ = connector(&profile(json!({})), requests.clone(), |request| {
        let path = url::Url::parse(&request.url).unwrap().path().to_owned();
        udm_inventory_body(&path)
    });
    let inventory = connector_
        .inventory(&connection("univention"))
        .await
        .unwrap();
    let ids: Vec<&str> = inventory.objects.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["account-1", "group-1"]);
    let account = inventory
        .objects
        .iter()
        .find(|o| o.id == "account-1")
        .unwrap();
    assert_eq!(account.kind, "account");
    assert_eq!(account.version, "\"1\"");
    assert_eq!(
        account.properties["erpxConnector"]["dn"],
        "uid=ada,ou=Lab,dc=example,dc=test"
    );
    assert!(requests.lock().unwrap().iter().all(|request| {
        request
            .url
            .starts_with("https://udm.example.test/univention/udm/")
    }));
    assert!(requests.lock().unwrap().iter().all(|request| {
        request.basic.as_ref().is_some_and(|(user, password)| {
            user == "administrator" && password == "directory-secret"
        })
    }));
}

#[tokio::test]
async fn active_directory_inventory_canonicalizes_approved_aliases() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let document = profile(json!({
        "windowsRunnerUrl": "https://runner.example.test/",
        "windowsRunnerToken": "w".repeat(32),
    }));
    let mut connector_ = connector(&document, requests.clone(), |request| {
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.bearer.as_deref(),
            Some("wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww")
        );
        assert_eq!(
            request.body.as_ref().unwrap()["connectionId"],
            "connection-1"
        );
        reply(
            200,
            json!({
                "objects": [
                    {
                        "id": "windows-guid", "dn": "CN=Ada,OU=Lab,DC=example,DC=test",
                        "name": "Ada", "kind": "account", "parentId": null,
                        "version": "7", "module": "user",
                        "properties": {"displayName": "Ada"}, "memberIds": []
                    },
                    {
                        "id": "gpo-guid", "dn": "CN=Baseline,OU=Lab,DC=example,DC=test",
                        "name": "Baseline", "kind": "gpo", "parentId": null,
                        "version": "3", "module": "gpo",
                        "properties": {"securityFilterGroupIds": ["windows-guid"]}, "memberIds": []
                    }
                ],
                "fields": [],
                "providerVersion": "runner-1",
                "observedAt": now(),
                "complete": true
            }),
        )
    });
    let inventory = connector_
        .inventory(&connection("active_directory"))
        .await
        .unwrap();
    let account = inventory
        .objects
        .iter()
        .find(|o| o.id == "account-1")
        .unwrap();
    assert_eq!(account.properties["windowsIdentity"]["id"], "windows-guid");
    assert_eq!(
        account.properties["windowsIdentity"]["dn"],
        "CN=Ada,OU=Lab,DC=example,DC=test"
    );
    let gpo = inventory
        .objects
        .iter()
        .find(|o| o.id == "gpo-guid")
        .unwrap();
    assert_eq!(
        gpo.properties["securityFilterGroupIds"],
        json!(["account-1"])
    );
}

fn inventory() -> Inventory {
    Inventory {
        observed_at: 500,
        objects: vec![
            InventoryObject {
                id: "gpo-1".into(),
                kind: "gpo".into(),
                version: "1".into(),
                properties: json!({"securityFilterGroupIds": []}),
                member_ids: vec![],
            },
            InventoryObject {
                id: "group-1".into(),
                kind: "group".into(),
                version: "\"2\"".into(),
                properties: json!({
                    "erpxConnector": {
                        "dn": "cn=Team,ou=Lab,dc=example,dc=test",
                        "module": "groups/group",
                        "name": "Team"
                    },
                    "users": []
                }),
                member_ids: vec!["account-1".into()],
            },
            InventoryObject {
                id: "account-1".into(),
                kind: "account".into(),
                version: "3".into(),
                properties: json!({
                    "erpxConnector": {
                        "dn": "uid=ada,ou=Lab,dc=example,dc=test",
                        "module": "users/user",
                        "name": "ada"
                    }
                }),
                member_ids: vec![],
            },
        ],
    }
}

#[tokio::test]
async fn write_operations_and_targets_are_gated_before_any_request() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let document = profile(json!({"writeOperations": []}));
    let mut connector_ = connector(&document, requests.clone(), |_| {
        panic!("no provider request before write approval")
    });
    let error = connector_
        .apply(
            &connection("univention"),
            "operation-1",
            &GpoAction::Filter {
                object_id: "gpo-1".into(),
                group_ids: vec!["group-1".into()],
            },
            &inventory(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "DIRECTORY_WRITE_NOT_APPROVED");

    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut connector_ = connector(&profile(json!({})), requests, |_| {
        panic!("no provider request for an invalid target")
    });
    let mut before = inventory();
    before.objects[1].kind = "ou".into();
    let error = connector_
        .apply(
            &connection("univention"),
            "operation-1",
            &GpoAction::Filter {
                object_id: "gpo-1".into(),
                group_ids: vec!["group-1".into()],
            },
            &before,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "DIRECTORY_TARGET_INVALID");
}

#[tokio::test]
async fn gpo_filter_uses_the_runner_with_bearer_and_no_implicit_scope() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let document = profile(json!({
        "sambaRunnerUrl": "https://runner.example.test/",
        "sambaRunnerToken": "s".repeat(32),
        "writeOperations": ["gpo.filter"]
    }));
    let mut connector_ = connector(&document, requests.clone(), |request| {
        assert!(request.url.ends_with("/apply"));
        assert_eq!(
            request.bearer.as_deref(),
            Some("ssssssssssssssssssssssssssssssss")
        );
        let body = request.body.as_ref().unwrap();
        assert_eq!(body["operationId"], "operation-1");
        assert_eq!(body["action"]["kind"], "gpo.filter");
        assert_eq!(body["action"]["groupIds"], json!(["group-1"]));
        assert_eq!(body["registry"], json!([]));
        assert_eq!(body["preferences"], json!([]));
        reply(200, json!({"verified": true}))
    });
    connector_
        .apply(
            &connection("univention"),
            "operation-1",
            &GpoAction::Filter {
                object_id: "gpo-1".into(),
                group_ids: vec!["group-1".into()],
            },
            &inventory(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn runner_directory_codes_map_to_owner_conflicts() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let document = profile(json!({
        "sambaRunnerUrl": "https://runner.example.test/",
        "sambaRunnerToken": "s".repeat(32),
        "writeOperations": ["gpo.filter"]
    }));
    let mut connector_ = connector(&document, requests, |_| {
        reply(409, json!({"error": {"code": "DIRECTORY_VERSION_CHANGED"}}))
    });
    let error = connector_
        .apply(
            &connection("univention"),
            "operation-1",
            &GpoAction::Filter {
                object_id: "gpo-1".into(),
                group_ids: vec!["group-1".into()],
            },
            &inventory(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "DIRECTORY_VERSION_CHANGED");
}

#[tokio::test]
async fn udm_group_membership_confirms_its_own_read_back() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let applied = Arc::new(Mutex::new(false));
    let state = applied.clone();
    let mut connector_ = connector(&profile(json!({})), requests.clone(), move |request| {
        let path = url::Url::parse(&request.url).unwrap().path().to_owned();
        if request.method == "PATCH" {
            assert_eq!(request.if_match.as_deref(), Some("\"2\""));
            assert_eq!(
                request.body.as_ref().unwrap()["properties"]["users"],
                json!(["uid=ada,ou=Lab,dc=example,dc=test"])
            );
            *state.lock().unwrap() = true;
            return reply(200, json!({}));
        }
        assert!(path.contains("groups/group/"));
        let users = if *state.lock().unwrap() {
            json!(["uid=ada,ou=Lab,dc=example,dc=test"])
        } else {
            json!([])
        };
        let mut object = reply(
            200,
            json!({
                "uuid": "group-1",
                "dn": "cn=Team,ou=Lab,dc=example,dc=test",
                "properties": {"name": "Team", "users": users}
            }),
        );
        object.etag = Some("\"2\"".into());
        object
    });
    connector_
        .apply(
            &connection("univention"),
            "operation-1",
            &GpoAction::Members {
                object_id: "group-1".into(),
                member_ids: vec!["account-1".into()],
                add: true,
            },
            &inventory(),
        )
        .await
        .unwrap();
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .any(|request| request.method == "PATCH")
    );
}

#[tokio::test]
async fn udm_group_membership_refuses_a_stale_version() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut connector_ = connector(&profile(json!({})), requests, |_| {
        let mut object = reply(
            200,
            json!({
                "uuid": "group-1",
                "dn": "cn=Team,ou=Lab,dc=example,dc=test",
                "properties": {"name": "Team", "users": []}
            }),
        );
        object.etag = Some("\"1\"".into());
        object
    });
    let error = connector_
        .apply(
            &connection("univention"),
            "operation-1",
            &GpoAction::Members {
                object_id: "group-1".into(),
                member_ids: vec!["account-1".into()],
                add: true,
            },
            &inventory(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "DIRECTORY_VERSION_CHANGED");
}

#[tokio::test]
async fn univention_requires_the_udm_profile_and_a_bound_write_dn() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let document = profile(json!({"udmUrl": null}));
    let mut connector_ = connector(&document, requests, |_| reply(200, json!({})));
    assert_eq!(
        connector_
            .inventory(&connection("univention"))
            .await
            .unwrap_err()
            .code,
        "UDM_PROFILE_REQUIRED"
    );

    let requests = Arc::new(Mutex::new(Vec::new()));
    let document = profile(json!({"writeBaseDns": []}));
    let mut connector_ = connector(&document, requests, |_| reply(200, json!({})));
    assert_eq!(
        connector_
            .apply(
                &connection("univention"),
                "operation-1",
                &GpoAction::Members {
                    object_id: "group-1".into(),
                    member_ids: vec!["account-1".into()],
                    add: true,
                },
                &inventory(),
            )
            .await
            .unwrap_err()
            .code,
        "DIRECTORY_WRITE_SCOPE_REQUIRED"
    );
}
