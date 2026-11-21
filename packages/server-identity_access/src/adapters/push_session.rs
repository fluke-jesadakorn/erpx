//! Background push admission uses persisted session identity, never browser proof
//! or an authority grant. Composition retains Control and Holding transactions.
use sqlx::PgConnection;

/// Check the native Holding child and its current selected Account parent.
/// The exact audience comes from the registered Holding, not the subscription.
pub async fn current_account(
    c: &mut PgConnection,
    session: &str,
    user: &str,
    audience: &str,
    environment: &str,
    now: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(r#"SELECT EXISTS(
      SELECT 1 FROM session child JOIN sso_session_bindings cb ON cb.session_id=child.id
      JOIN session parent ON parent.id=cb.parent_session_id AND parent."userId"=child."userId"
      JOIN sso_session_bindings pb ON pb.session_id=parent.id
      JOIN account_device_accounts a ON a.session_id=parent.id
      JOIN account_device_browsers device ON device.id=a.browser_id AND device.active_session_id=parent.id
      JOIN "user" u ON u.id=child."userId" LEFT JOIN principals p ON p.user_id=u.id
      WHERE child.id=$1 AND child."userId"=$2 AND cb.audience=$3 AND cb.environment=$4 AND pb.environment=$4
      AND pb.audience='account' AND pb.parent_session_id IS NULL AND COALESCE(p.status,'active')='active'
      AND child."expiresAt"::timestamptz>to_timestamp($5::double precision/1000)
      AND parent."expiresAt"::timestamptz>to_timestamp($5::double precision/1000)
      AND pb.authenticated_at<=$5 AND pb.absolute_expires_at>$5 AND pb.absolute_expires_at<=pb.authenticated_at+43200000
      AND cb.authenticated_at=pb.authenticated_at AND cb.absolute_expires_at>$5
      AND cb.absolute_expires_at<=cb.authenticated_at+43200000 AND cb.absolute_expires_at<=pb.absolute_expires_at
    )"#).bind(session).bind(user).bind(audience).bind(environment).bind(now).fetch_one(c).await
}

/// A live selected Company context is required even for a content-free wake-up.
pub async fn current_company(
    c: &mut PgConnection,
    session: &str,
    user: &str,
    company: &str,
    now: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT EXISTS(
      SELECT 1 FROM active_work_contexts c JOIN assignments a
      ON a.id=c.assignment_id AND a.company_id=c.company_id AND a.user_id=c.user_id
      WHERE c.session_id=$1 AND c.user_id=$2 AND c.company_id=$3 AND c.expires_at>$4
      AND a.version=c.assignment_version AND a.revoked_at IS NULL
      AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4)
      AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.session_id=c.session_id
        AND r.company_id=c.company_id AND r.user_id=c.user_id)
    )"#,
    )
    .bind(session)
    .bind(user)
    .bind(company)
    .bind(now)
    .fetch_one(c)
    .await
}
