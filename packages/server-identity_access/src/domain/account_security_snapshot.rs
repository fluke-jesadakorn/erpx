//! Display/revision projection only. Copied snapshots never prove authentication;
//! every mutation reloads them under the session-first Control lease.
use super::account_security::{SecurityFamily, SecuritySeed};
use erpx_server_shared::public::BusinessError;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
#[derive(Clone, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub user_id: String,
    pub session_id: String,
}
#[derive(Clone, PartialEq, serde::Deserialize, Serialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub user_agent: Option<String>,
    pub image: String,
    #[serde(skip_serializing)]
    pub expires_ms: i64,
}
#[derive(Clone, PartialEq, serde::Deserialize, Serialize)]
pub struct Binding {
    pub session_id: String,
    pub parent_session_id: Option<String>,
    pub audience: String,
    pub environment: String,
    pub authenticated_at: i64,
    pub absolute_expires_at: i64,
    pub image: String,
}
#[derive(Clone, PartialEq, serde::Deserialize, Serialize)]
pub struct Mfa {
    pub enabled: bool,
    pub image: String,
}
#[derive(Clone, PartialEq, serde::Deserialize, Serialize)]
pub struct Verification {
    pub session_id: String,
    pub user_id: String,
    pub verified_at: i64,
    pub image: String,
}
#[derive(Clone, PartialEq, serde::Deserialize, Serialize)]
pub struct Image {
    pub sessions: Vec<Session>,
    pub bindings: Vec<Binding>,
    pub mfa: Option<Mfa>,
    pub verifications: Vec<Verification>,
    pub user: Option<String>,
}
pub fn digest(value: &impl Serialize) -> Result<String, BusinessError> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value).map_err(|_| {
            BusinessError::invalid(
                "INVALID_SECURITY_SOURCE",
                "The security source could not be encoded.",
            )
        })?,
    )))
}
pub fn family_handle(actor: &Actor, parent: &str) -> Result<String, BusinessError> {
    digest(&("account-family", &actor.user_id, &actor.session_id, parent))
}
pub fn seed(image: &Image, actor: &Actor, now: i64) -> Result<SecuritySeed, BusinessError> {
    let own = image
        .sessions
        .iter()
        .filter(|s| s.user_id == actor.user_id)
        .collect::<Vec<_>>();
    let mut families = Vec::new();
    for row in &own {
        let Some(binding) = image.bindings.iter().find(|b| b.session_id == row.id) else {
            continue;
        };
        if binding.parent_session_id.is_some()
            || binding.audience != "account"
            || binding.environment != "development"
            || binding.authenticated_at > now
            || binding.absolute_expires_at <= now
            || binding.absolute_expires_at > binding.authenticated_at.saturating_add(43_200_000)
            || row.expires_ms <= now
        {
            continue;
        }
        let audiences = image
            .bindings
            .iter()
            .filter(|b| {
                b.parent_session_id.as_deref() == Some(row.id.as_str())
                    && b.environment == "development"
                    && own.iter().any(|s| s.id == b.session_id)
            })
            .map(|b| b.audience.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        families.push(SecurityFamily {
            handle: family_handle(actor, &row.id)?,
            current: row.id == actor.session_id,
            created_at: binding.authenticated_at,
            expires_at: row.expires_ms.min(binding.absolute_expires_at),
            user_agent: row.user_agent.as_ref().map(|v| {
                v.chars()
                    .scan(0, |length, c| {
                        *length += c.len_utf16();
                        (*length <= 500).then_some(c)
                    })
                    .collect()
            }),
            audiences,
        });
    }
    #[derive(Serialize)]
    struct Revision<'a> {
        actor: &'a Actor,
        sessions: &'a Vec<&'a Session>,
        bindings: Vec<&'a Binding>,
        mfa: &'a Option<Mfa>,
        verifications: &'a Vec<Verification>,
        user: &'a Option<String>,
    }
    let revision = digest(&Revision {
        actor,
        sessions: &own,
        bindings: image
            .bindings
            .iter()
            .filter(|b| {
                own.iter().any(|s| {
                    s.id == b.session_id || b.parent_session_id.as_deref() == Some(s.id.as_str())
                })
            })
            .collect(),
        mfa: &image.mfa,
        verifications: &image.verifications,
        user: &image.user,
    })?;
    Ok(SecuritySeed {
        account_key: digest(&("account-security", &actor.user_id, &actor.session_id))?,
        revision,
        mfa_enabled: image.mfa.as_ref().is_some_and(|m| m.enabled),
        mfa_fresh_until: image
            .verifications
            .iter()
            .find(|v| v.session_id == actor.session_id && v.user_id == actor.user_id)
            .filter(|_| image.mfa.as_ref().is_some_and(|m| m.enabled))
            .map(|v| v.verified_at.saturating_add(300_000)),
        families,
        legacy_sessions: own
            .iter()
            .filter(|s| s.expires_ms > now && !image.bindings.iter().any(|b| b.session_id == s.id))
            .count(),
    })
}
