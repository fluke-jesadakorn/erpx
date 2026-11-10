//! Read-only Control identity for one presented product session.
//!
//! The opaque token must belong to a live child SSO session whose parent is the
//! selected Account session for the same user. No cookie, header or Better Auth
//! provider call is accepted; the caller supplies the exact token and audience.
use crate::domain::{
    account_auth::SESSION_LIFETIME_MS,
    environment::valid_environment,
    portal_session::{
        session_required, valid_audience, valid_session_token, PortalSessionProof,
        PortalSessionReadRequest,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
use sqlx::Row;

/// Failure to observe the session is indistinguishable from no session at the
/// business boundary. Malformed tokens and unknown audiences are rejected
/// before SQL; a missing row remains `Ok(None)`.
pub async fn verify_portal_session(
    connection: &mut PgConnection,
    request: &PortalSessionReadRequest<'_>,
    now: i64,
) -> Result<Option<PortalSessionProof>, PgTransactionError<BusinessError>> {
    if !valid_environment(request.environment) {
        return Err(BusinessError::invalid(
            "ACCOUNT_ENVIRONMENT_UNAVAILABLE",
            "This Account binding environment is not enabled.",
        )
        .into());
    }
    if now <= 0 || !valid_session_token(request.token) {
        return Ok(None);
    }
    if let Some(audience) = request.audience {
        if !valid_audience(audience) {
            return Ok(None);
        }
        return verify_audience_session(connection, request, audience, now).await;
    }
    verify_direct_session(connection, request, now).await
}

async fn verify_audience_session(
    connection: &mut PgConnection,
    request: &PortalSessionReadRequest<'_>,
    audience: &str,
    now: i64,
) -> Result<Option<PortalSessionProof>, PgTransactionError<BusinessError>> {
    let row = sqlx::query(
        r#"SELECT s.id session_id,s."userId" user_id,u.email,u."emailVerified" email_verified,
          cb.authenticated_at,
          LEAST(cb.absolute_expires_at,
            (EXTRACT(EPOCH FROM s."expiresAt"::timestamptz)*1000)::bigint,
            (EXTRACT(EPOCH FROM parent."expiresAt"::timestamptz)*1000)::bigint) expires_at
          FROM session s
          JOIN "user" u ON u.id=s."userId"
          LEFT JOIN principals p ON p.user_id=u.id
          JOIN sso_session_bindings cb ON cb.session_id=s.id
          JOIN session parent ON parent.id=cb.parent_session_id AND parent."userId"=s."userId"
          JOIN sso_session_bindings pb ON pb.session_id=parent.id
          JOIN account_device_accounts device ON device.session_id=parent.id
          JOIN account_device_browsers browser ON browser.id=device.browser_id AND browser.active_session_id=parent.id
          WHERE s.token=$1 AND cb.audience=$2 AND cb.environment=$3
            AND cb.authenticated_at<=$4 AND cb.absolute_expires_at>$4
            AND cb.absolute_expires_at<=cb.authenticated_at+$5
            AND pb.audience='account' AND pb.environment=$3 AND pb.parent_session_id IS NULL
            AND pb.authenticated_at<=$4 AND pb.authenticated_at=cb.authenticated_at
            AND pb.absolute_expires_at>$4 AND pb.absolute_expires_at<=pb.authenticated_at+$5
            AND pb.absolute_expires_at>=cb.absolute_expires_at
            AND s."expiresAt"::timestamptz>to_timestamp($4::double precision/1000)
            AND parent."expiresAt"::timestamptz>to_timestamp($4::double precision/1000)
            AND COALESCE(p.status,'active')='active'"#,
    )
    .bind(request.token)
    .bind(audience)
    .bind(request.environment)
    .bind(now)
    .bind(SESSION_LIFETIME_MS)
    .fetch_optional(&mut *connection)
    .await?;
    row.map(|row| {
        Ok::<PortalSessionProof, sqlx::Error>(PortalSessionProof {
            session_id: row.try_get("session_id")?,
            user_id: row.try_get("user_id")?,
            email: row.try_get("email")?,
            email_verified: row.try_get::<i64, _>("email_verified")? == 1,
            authenticated_at: row.try_get("authenticated_at")?,
            expires_at: row.try_get("expires_at")?,
        })
    })
    .transpose()
    .map_err(Into::into)
}

async fn verify_direct_session(
    connection: &mut PgConnection,
    request: &PortalSessionReadRequest<'_>,
    now: i64,
) -> Result<Option<PortalSessionProof>, PgTransactionError<BusinessError>> {
    let row = sqlx::query(
        r#"SELECT s.id session_id,s."userId" user_id,u.email,u."emailVerified" email_verified,
          (EXTRACT(EPOCH FROM s."createdAt"::timestamptz)*1000)::bigint authenticated_at,
          (EXTRACT(EPOCH FROM s."expiresAt"::timestamptz)*1000)::bigint expires_at
          FROM session s
          JOIN "user" u ON u.id=s."userId"
          LEFT JOIN principals p ON p.user_id=u.id
          WHERE s.token=$1
            AND s."expiresAt"::timestamptz>to_timestamp($2::double precision/1000)
            AND COALESCE(p.status,'active')='active'
            AND NOT EXISTS(SELECT 1 FROM sso_session_bindings b WHERE b.session_id=s.id)"#,
    )
    .bind(request.token)
    .bind(now)
    .fetch_optional(&mut *connection)
    .await?;
    row.map(|row| {
        Ok::<PortalSessionProof, sqlx::Error>(PortalSessionProof {
            session_id: row.try_get("session_id")?,
            user_id: row.try_get("user_id")?,
            email: row.try_get("email")?,
            email_verified: row.try_get::<i64, _>("email_verified")? == 1,
            authenticated_at: row.try_get("authenticated_at")?,
            expires_at: row.try_get("expires_at")?,
        })
    })
    .transpose()
    .map_err(Into::into)
}

/// Defense in depth for a composition guard that already accepted a proof.
/// It rejects a token whose session, parent, SSO binding, principal or email
/// changed between the initial resolution and the owner commit.
pub async fn require_current_portal_session(
    connection: &mut PgConnection,
    request: &PortalSessionReadRequest<'_>,
    expected: &PortalSessionProof,
    now: i64,
) -> Result<PortalSessionProof, PgTransactionError<BusinessError>> {
    let current = verify_portal_session(connection, request, now).await?;
    match current {
        Some(current)
            if current.session_id == expected.session_id
                && current.user_id == expected.user_id
                && current.email.eq_ignore_ascii_case(&expected.email)
                && current.email_verified
                && current.expires_at > now =>
        {
            Ok(current)
        }
        _ => Err(session_required().into()),
    }
}

#[cfg(test)]
#[path = "../../../../tests/support/rust-postgres.rs"]
mod fixture;

#[cfg(test)]
mod tests {
    use super::fixture;
    use super::*;
    use erpx_server_runtime::public::postgres::{
        load_migrations, migrate_database, DatabasePool, MigrationTrack,
    };

    const NOW: i64 = 1_800_000_000_000;
    const AUDIENCE: &str = "portal";
    const ENVIRONMENT: &str = "development";

    fn read(token: &str) -> PortalSessionReadRequest<'_> {
        PortalSessionReadRequest {
            token,
            audience: Some(AUDIENCE),
            environment: ENVIRONMENT,
        }
    }

    async fn setup(pool: &DatabasePool) {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files = load_migrations(root, MigrationTrack::Control)
            .await
            .unwrap();
        migrate_database(pool, MigrationTrack::Control, &files)
            .await
            .unwrap();
        sqlx::raw_sql(
            r#"
            INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES
              ('user-valid','Valid','valid@example.test',1,100,100),
              ('user-unverified','Unverified','unverified@example.test',0,100,100),
              ('user-suspended','Suspended','suspended@example.test',1,100,100),
              ('user-expired','Expired','expired@example.test',1,100,100),
              ('user-nodevice','No Device','nodevice@example.test',1,100,100),
              ('user-wrong-parent','Wrong Parent','wrong-parent@example.test',1,100,100),
              ('user-direct','Direct','direct@example.test',1,100,100);
            INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES
              ('principal-suspended','user-suspended','Suspended','suspended',100);
            INSERT INTO session(id,"expiresAt",token,"createdAt","updatedAt","ipAddress","userAgent","userId") VALUES
              ('parent-valid',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-valid-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-valid'),
              ('child-valid',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-valid-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-valid'),
              ('parent-unverified',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-unverified-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-unverified'),
              ('child-unverified',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-unverified-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-unverified'),
              ('parent-suspended',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-suspended-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-suspended'),
              ('child-suspended',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-suspended-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-suspended'),
              ('parent-expired',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-expired-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-expired'),
              ('child-expired',to_char(to_timestamp(1799999999.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-expired-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-expired'),
              ('parent-nodevice',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-nodevice-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-nodevice'),
              ('child-nodevice',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-nodevice-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-nodevice'),
              ('parent-wrong-parent',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-wrong-parent-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-wrong-parent'),
              ('child-wrong-parent',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-wrong-parent-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-wrong-parent'),
              ('direct',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'direct-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-direct');
            INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES
              ('parent-valid','account','development',NULL,1799999999000,1800000060000),
              ('child-valid','portal','development','parent-valid',1799999999000,1800000060000),
              ('parent-unverified','account','development',NULL,1799999999000,1800000060000),
              ('child-unverified','portal','development','parent-unverified',1799999999000,1800000060000),
              ('parent-suspended','account','development',NULL,1799999999000,1800000060000),
              ('child-suspended','portal','development','parent-suspended',1799999999000,1800000060000),
              ('parent-expired','account','development',NULL,1799999999000,1800000060000),
              ('child-expired','portal','development','parent-expired',1799999999000,1800000060000),
              ('parent-nodevice','account','development',NULL,1799999999000,1800000060000),
              ('child-nodevice','portal','development','parent-nodevice',1799999999000,1800000060000),
              ('parent-wrong-parent','portal','development',NULL,1799999999000,1800000060000),
              ('child-wrong-parent','portal','development','parent-wrong-parent',1799999999000,1800000060000);
            INSERT INTO account_device_browsers(id,active_session_id,revision,updated_at) VALUES
              ('browser-valid','parent-valid',1,100),
              ('browser-unverified','parent-unverified',1,100),
              ('browser-suspended','parent-suspended',1,100),
              ('browser-expired','parent-expired',1,100),
              ('browser-nodevice',NULL,1,100),
              ('browser-wrong-parent','parent-wrong-parent',1,100);
            INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES
              ('parent-valid','browser-valid','handle-valid'),
              ('parent-unverified','browser-unverified','handle-unverified'),
              ('parent-suspended','browser-suspended','handle-suspended'),
              ('parent-expired','browser-expired','handle-expired'),
              ('parent-nodevice','browser-nodevice','handle-nodevice'),
              ('parent-wrong-parent','browser-wrong-parent','handle-wrong-parent');
            "#,
        )
        .execute(pool.raw())
        .await
        .unwrap();
    }

    async fn resolve(pool: &DatabasePool, token: &str) -> Option<PortalSessionProof> {
        let mut connection = pool.raw().acquire().await.unwrap();
        verify_portal_session(&mut connection, &read(token), NOW)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn verifies_live_child_session_and_reports_verification_state() {
        if std::env::var_os("ERPX_TEST_ADMIN_URL").is_none() {
            return;
        }
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;

            let proof = resolve(&pool, "child-valid-token")
                .await
                .expect("live child session resolves");
            assert_eq!(proof.session_id, "child-valid");
            assert_eq!(proof.user_id, "user-valid");
            assert_eq!(proof.email, "valid@example.test");
            assert!(proof.email_verified);
            assert!(proof.expires_at > NOW);
            assert_eq!(proof.authenticated_at, 1799999999000);

            assert!(resolve(&pool, "missing-token").await.is_none());
            assert!(resolve(&pool, "child token").await.is_none());

            let unverified = resolve(&pool, "child-unverified-token")
                .await
                .expect("identity read still reports an unverified session fact");
            assert!(!unverified.email_verified);

            assert!(resolve(&pool, "child-suspended-token").await.is_none());
            assert!(resolve(&pool, "child-expired-token").await.is_none());
            assert!(resolve(&pool, "child-nodevice-token").await.is_none());
            assert!(
                verify_portal_session(
                    &mut pool.raw().acquire().await.unwrap(),
                    &PortalSessionReadRequest {
                        audience: Some("other-product"),
                        ..read("child-valid-token")
                    },
                    NOW,
                )
                .await
                .unwrap()
                .is_none(),
                "a session bound to another audience must not resolve"
            );
            assert!(
                resolve(&pool, "child-wrong-parent-token").await.is_none(),
                "the parent SSO binding must be the selected Account session"
            );

            let direct = verify_portal_session(
                &mut pool.raw().acquire().await.unwrap(),
                &PortalSessionReadRequest {
                    audience: None,
                    ..read("direct-token")
                },
                NOW,
            )
            .await
            .unwrap()
            .expect("the direct Account session mode remains supported");
            assert_eq!(direct.user_id, "user-direct");
            assert!(direct.email_verified);
            assert!(
                verify_portal_session(
                    &mut pool.raw().acquire().await.unwrap(),
                    &PortalSessionReadRequest {
                        audience: None,
                        ..read("child-valid-token")
                    },
                    NOW,
                )
                .await
                .unwrap()
                .is_none(),
                "an SSO child session is not a direct Account session"
            );
        })
        .await;
    }
}
