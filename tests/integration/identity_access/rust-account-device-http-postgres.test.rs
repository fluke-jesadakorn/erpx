#[allow(
    dead_code,
    reason = "shared actual Account HTTP fixtures support multiple native suites"
)]
#[path = "../../support/rust-account-device-http.rs"]
mod http;
use axum::http::{StatusCode, header};
use axum::{body::Body, http::Request};
use erpx_server_identity_access::public::account_auth_router;
use http::revocation as fixture;
use http::*;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn exact_holding_drawer_signin_returns_the_existing_envelope_and_host_only_opaque_cookies() {
    fixture::with_system(|system|async move{
 let router=account_auth_router(system.scenario.auth.clone());let mut jar=Jar::default();let state=sign_in(&router,&mut jar,"a@example.test",None,ACME).await;
 assert_eq!(state.as_object().unwrap().len(),4);assert_eq!(state["revision"],1);assert_eq!(state["accounts"][0]["current"],true);assert_eq!(jar.0.len(),3);no_credentials(&state,&jar);
 let(status,body,headers)=send(&router,&mut jar,"GET",DEVICE,Some(NORTHSTAR),None).await;assert_eq!(status,StatusCode::OK);assert_eq!(body,json!({"data":state}));assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_CREDENTIALS],"true");assert!(headers[header::CACHE_CONTROL].to_str().unwrap().contains("no-store"));assert_eq!(headers[header::REFERRER_POLICY],"no-referrer");no_credentials(&body,&jar);
 let(status,_,_)=send(&router,&mut jar,"POST","/api/auth/sign-in/email",Some(ACME),Some(json!({"email":"b@example.test","password":fixture::fixture::PASSWORD,"revision":1}))).await;assert_eq!(status,StatusCode::FORBIDDEN);
 let(status,body,headers)=send(&router,&mut jar,"POST","/api/auth/sign-in/email",Some(ACCOUNT),Some(json!({"email":"b@example.test","password":fixture::fixture::PASSWORD,"revision":1,"rememberMe":false}))).await;assert_eq!(status,StatusCode::OK);assert!(body.get("user").is_some());assert!(body.get("data").is_none());
 for cookie in headers.get_all(header::SET_COOKIE){let cookie=cookie.to_str().unwrap();assert!(cookie.contains("Secure; HttpOnly; SameSite=Lax"));assert!(!cookie.contains("Domain="));if cookie.starts_with(ACTIVE){assert!(!cookie.contains("Max-Age="));}}
}).await;
}

#[tokio::test]
async fn anonymous_discovery_and_initialize_keep_the_existing_data_contract() {
    fixture::with_system(|system| async move {
        let router = account_auth_router(system.scenario.auth.clone());
        let mut jar = Jar::default();
        let (status, body, _) = send(&router, &mut jar, "GET", DEVICE, None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({"data":{"initialized":false,"revision":0,"maximum":5,"accounts":[]}})
        );
        let (status, body, headers) = send(
            &router,
            &mut jar,
            "POST",
            &format!("{DEVICE}/initialize"),
            Some(ACME),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({"data":{"initialized":true,"revision":0,"maximum":5,"accounts":[]}})
        );
        assert_eq!(jar.0.len(), 1);
        assert!(jar.0.contains_key(GROUP));
        assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], ACME);
    })
    .await;
}

#[tokio::test]
async fn drawer_add_switch_and_inactive_remove_use_real_two_holding_fences_and_revision_cas() {
    fixture::with_system(|system| async move {
        let router = account_auth_router(system.scenario.auth.clone());
        let mut jar = Jar::default();
        let a = sign_in(&router, &mut jar, "a@example.test", None, ACME).await;
        let handle_a = a["accounts"][0]["handle"].as_str().unwrap().to_owned();
        let retired = fixture::children(&system, &jar.proof(), "fixture-a").await;
        let b = sign_in(&router, &mut jar, "b@example.test", Some(1), NORTHSTAR).await;
        assert_eq!(b["revision"], 2);
        fixture::assert_fenced(&system, &retired, "fixture-a").await;
        let handle_b = b["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["current"] == true)
            .unwrap()["handle"]
            .as_str()
            .unwrap()
            .to_owned();
        let (status, body, _) = send(
            &router,
            &mut jar,
            "POST",
            &format!("{DEVICE}/active"),
            Some(ACME),
            Some(json!({"handle":handle_a,"revision":2})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["revision"], 3);
        let current = fixture::children(&system, &jar.proof(), "fixture-a").await;
        let active = jar.0[ACTIVE].clone();
        let (status, body, _) = send(
            &router,
            &mut jar,
            "DELETE",
            DEVICE,
            Some(ACME),
            Some(json!({"handle":handle_b,"revision":3})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["revision"], 4);
        assert_eq!(jar.0[ACTIVE], active);
        assert!(!jar.0.contains_key(&format!("{SAVED}{handle_b}")));
        for (pool, child) in system.holdings.iter().zip(&current) {
            assert!(!fixture::fenced(pool, child, "fixture-a").await);
        }
        no_credentials(&body, &jar);
    })
    .await;
}

#[tokio::test]
async fn removing_current_account_clears_active_cookie_even_when_its_saved_cookie_is_missing() {
    fixture::with_system(|system| async move {
        let router = account_auth_router(system.scenario.auth.clone());
        let mut jar = Jar::default();
        let a = sign_in(&router, &mut jar, "a@example.test", None, ACME).await;
        let handle = a["accounts"][0]["handle"].as_str().unwrap();
        jar.0.remove(&format!("{SAVED}{handle}"));
        let (status, body, headers) = send(
            &router,
            &mut jar,
            "DELETE",
            DEVICE,
            Some(ACME),
            Some(json!({"handle":handle,"revision":1})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["accounts"], json!([]));
        assert_eq!(body["data"]["revision"], 2);
        assert!(!jar.0.contains_key(ACTIVE));
        assert!(
            headers
                .get_all(header::SET_COOKIE)
                .iter()
                .any(|cookie| cookie.to_str().unwrap().starts_with(&format!("{ACTIVE}=;")))
        );
    })
    .await;
}

#[tokio::test]
async fn preflight_accepts_only_registered_origins_and_the_declared_cookie_json_transport() {
    fixture::with_system(|system| async move {
        let router = account_auth_router(system.scenario.auth.clone());
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri(DEVICE)
                    .header("host", "account.example.test")
                    .header("origin", ACME)
                    .header("access-control-request-method", "POST")
                    .header("access-control-request-headers", "Content-Type")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            ACME
        );
        assert_eq!(response.headers()[header::ACCESS_CONTROL_MAX_AGE], "600");
        assert!(!response.headers().contains_key(header::SET_COOKIE));
        for origin in [
            "null",
            "http://acme.example.test",
            "https://acme.example.test:444",
            "https://acme.example.test.evil.test",
            "https://account.example.test.evil.test",
            "https://apps.example.test",
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("OPTIONS")
                        .uri(DEVICE)
                        .header("host", "account.example.test")
                        .header("origin", origin)
                        .header("access-control-request-method", "POST")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            assert!(
                !response
                    .headers()
                    .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            );
        }
        let response = router
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri(DEVICE)
                    .header("host", "account.example.test")
                    .header("origin", ACME)
                    .header("access-control-request-method", "POST")
                    .header("access-control-request-headers", "Authorization")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            !response
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_HEADERS)
        );
    })
    .await;
}

#[tokio::test]
async fn exact_authority_and_closed_selection_json_reject_injected_roles_and_forwarded_host_spoofing()
 {
    fixture::with_system(|system|async move{
 let router=account_auth_router(system.scenario.auth.clone());let mut jar=Jar::default();let a=sign_in(&router,&mut jar,"a@example.test",None,ACME).await;let handle=a["accounts"][0]["handle"].as_str().unwrap();
 for body in [json!({"handle":handle,"revision":1,"role":"holding_owner"}),json!({"handle":handle,"revision":1,"authority":{"sessionId":"forged","revision":1,"assignmentVersion":1}})]{
  let(status,_,_)=send(&router,&mut jar,"POST",&format!("{DEVICE}/active"),Some(ACME),Some(body)).await;assert_eq!(status,StatusCode::BAD_REQUEST);
 }
 let response=router.oneshot(Request::builder().uri(DEVICE).header("host","unrelated.example.test").header("x-forwarded-host","account.example.test").header("origin",ACME).header("cookie",jar.header()).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::NOT_FOUND);assert!(!response.headers().contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));assert_eq!(system.scenario.auth.devices(&jar.proof()).await.unwrap().revision,1);
}).await;
}

#[tokio::test]
async fn failed_drawer_credentials_are_enumeration_safe_and_leave_current_work_untouched() {
    fixture::with_system(|system| async move {
        let router = account_auth_router(system.scenario.auth.clone());
        let mut jar = Jar::default();
        sign_in(&router, &mut jar, "a@example.test", None, ACME).await;
        let current = fixture::children(&system, &jar.proof(), "fixture-a").await;
        let mut observed = vec![];
        for email in ["a@example.test", "missing@example.test"] {
            let (status, body, headers) = send(
                &router,
                &mut jar,
                "POST",
                &format!("{DEVICE}/sign-in"),
                Some(ACME),
                Some(json!({"email":email,"password":"wrong-synthetic-password","revision":1})),
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            assert_eq!(body["error"]["code"], "SIGN_IN_FAILED");
            assert!(!headers.contains_key(header::SET_COOKIE));
            observed.push(body);
        }
        assert_eq!(observed[0], observed[1]);
        assert_eq!(
            system
                .scenario
                .auth
                .devices(&jar.proof())
                .await
                .unwrap()
                .revision,
            1
        );
        for (pool, child) in system.holdings.iter().zip(&current) {
            assert!(!fixture::fenced(pool, child, "fixture-a").await);
        }
    })
    .await;
}

#[tokio::test]
async fn partial_holding_failure_never_returns_success_data_or_new_auth_cookies() {
    fixture::with_system(|system|async move{
 let router=account_auth_router(system.scenario.auth.clone());let mut jar=Jar::default();sign_in(&router,&mut jar,"a@example.test",None,ACME).await;let retired=fixture::children(&system,&jar.proof(),"fixture-a").await;
 sqlx::query("ALTER TABLE active_work_contexts RENAME TO fixture_unavailable_contexts").execute(system.holdings[1].raw()).await.unwrap();
 let(status,body,headers)=send(&router,&mut jar,"POST",&format!("{DEVICE}/sign-in"),Some(ACME),Some(json!({"email":"b@example.test","password":fixture::fixture::PASSWORD,"revision":1}))).await;assert_eq!(status,StatusCode::SERVICE_UNAVAILABLE);assert_eq!(body["error"]["code"],"DEVICE_ACCOUNT_REENTRY_REQUIRED");assert!(body.get("data").is_none());assert!(!headers.contains_key(header::SET_COOKIE));assert_eq!(system.scenario.auth.devices(&jar.proof()).await.unwrap().revision,1);assert!(fixture::fenced(&system.holdings[0],&retired[0],"fixture-a").await);
 sqlx::query("ALTER TABLE fixture_unavailable_contexts RENAME TO active_work_contexts").execute(system.holdings[1].raw()).await.unwrap();
}).await;
}
