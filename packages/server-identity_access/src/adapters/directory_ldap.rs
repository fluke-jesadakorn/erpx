//! Actual bounded LDAPS provider. Provider I/O completes before any database
//! lease; no LDAP operation alone creates an ERPX user, membership or session.
use super::{
    directory_ldap_snapshot as row,
    directory_ldap_tls::LdapsTransportPolicy,
    directory_ldap_wire::{LdapFailure, ReferralPolicy, SequentialLdap},
    directory_profiles::{DirectoryProfileStore, DirectorySecretProfile},
};
use crate::{
    application::{
        account_ports::AccountFuture,
        directory::{DirectoryPassword, DirectoryProviderRead, VerifiedDirectorySubject},
    },
    domain::directory::*,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use ldap3_proto::{parse_ldap_filter_str, proto::*};
use std::{collections::BTreeSet, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct LdapDirectoryProvider {
    profiles: Arc<DirectoryProfileStore>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl LdapDirectoryProvider {
    pub fn new(profiles: Arc<DirectoryProfileStore>, clock: Arc<dyn Clock + Send + Sync>) -> Self {
        Self { profiles, clock }
    }
}
fn error(_: LdapFailure) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_PROVIDER_UNAVAILABLE",
        "The bounded Directory observation did not complete.",
    )
}
fn request(
    base: &str,
    scope: LdapSearchScope,
    filter: LdapFilter,
    attrs: &[&str],
    limit: i32,
) -> LdapSearchRequest {
    LdapSearchRequest {
        base: base.into(),
        scope,
        aliases: LdapDerefAliases::Never,
        sizelimit: limit,
        timelimit: 15,
        typesonly: false,
        filter,
        attrs: attrs.iter().map(|v| (*v).into()).collect(),
    }
}
fn user_filter(ad: bool, custom: Option<&str>) -> Result<LdapFilter, BusinessError> {
    if let Some(custom) = custom {
        if custom.len() > 16_384 {
            return Err(incomplete());
        }
        let mut depth = 0usize;
        let mut escaped = false;
        for ch in custom.chars() {
            if !escaped {
                if ch == '(' {
                    depth += 1;
                    if depth > 32 {
                        return Err(incomplete());
                    }
                } else if ch == ')' {
                    depth = depth.checked_sub(1).ok_or_else(incomplete)?;
                }
            }
            escaped = ch == '\\' && !escaped;
        }
        if depth != 0 || escaped {
            return Err(incomplete());
        }
        return tracing::subscriber::with_default(
            tracing::subscriber::NoSubscriber::default(),
            || parse_ldap_filter_str(custom),
        )
        .map_err(|_| incomplete());
    }
    Ok(if ad {
        LdapFilter::And(vec![
            LdapFilter::Equality("objectCategory".into(), "person".into()),
            LdapFilter::Equality("objectClass".into(), "user".into()),
        ])
    } else {
        LdapFilter::And(vec![
            LdapFilter::Equality("objectClass".into(), "posixAccount".into()),
            LdapFilter::Present("uid".into()),
        ])
    })
}
fn transport(
    connection: &DirectoryConnectionDescriptor,
    profile: &DirectorySecretProfile,
) -> Result<LdapsTransportPolicy, BusinessError> {
    if connection.config.mode != connection.mode
        || !matches!(
            connection.provider,
            DirectoryProvider::ActiveDirectory | DirectoryProvider::Univention
        )
    {
        return Err(incomplete());
    }
    LdapsTransportPolicy::checked(
        connection
            .config
            .ldap_url
            .as_deref()
            .ok_or_else(incomplete)?,
        &profile.allowed_hosts,
        profile.ca_pem.as_deref(),
        profile.tls_server_name.as_deref(),
    )
    .map_err(error)
}
async fn bind<S: AsyncRead + AsyncWrite + Unpin>(
    wire: &mut SequentialLdap<S>,
    profile: &DirectorySecretProfile,
) -> Result<(), BusinessError> {
    wire.bind(
        profile
            .bind_dn
            .as_ref()
            .filter(|v| !v.is_empty())
            .ok_or_else(incomplete)?
            .clone(),
        profile
            .bind_password
            .as_ref()
            .filter(|v| !v.is_empty())
            .ok_or_else(incomplete)?
            .clone(),
    )
    .await
    .map_err(error)
}

impl DirectoryProviderRead for LdapDirectoryProvider {
    fn verify<'a>(
        &'a self,
        connection: &'a DirectoryConnectionDescriptor,
        username: &'a str,
        password: DirectoryPassword,
    ) -> AccountFuture<'a, VerifiedDirectorySubject> {
        Box::pin(async move {
            let lease = self
                .profiles
                .retain(
                    &connection.config.secret_profile,
                    &connection.connection_id,
                    Some(&connection.profile_fingerprint),
                )
                .await?;
            let profile = lease.profile();
            let ad = connection.provider == DirectoryProvider::ActiveDirectory;
            let mut wire = transport(connection, profile)?
                .connect()
                .await
                .map_err(error)?;
            let result = async {
                bind(&mut wire, profile).await?;
                let filter = if ad {
                    LdapFilter::And(vec![
                        LdapFilter::Equality("objectCategory".into(), "person".into()),
                        LdapFilter::Equality("objectClass".into(), "user".into()),
                        LdapFilter::Or(vec![
                            LdapFilter::Equality("sAMAccountName".into(), username.into()),
                            LdapFilter::Equality("userPrincipalName".into(), username.into()),
                        ]),
                    ])
                } else {
                    LdapFilter::And(vec![
                        LdapFilter::Equality("objectClass".into(), "posixAccount".into()),
                        LdapFilter::Equality("uid".into(), username.into()),
                    ])
                };
                // Typed equality values are encoded by the maintained codec; a user
                // string cannot splice LDAP filter syntax or alter the search base.
                let rows = wire
                    .search(
                        request(
                            connection.config.base_dn.as_deref().unwrap_or(""),
                            LdapSearchScope::Subtree,
                            filter,
                            &[
                                "objectGUID",
                                "entryUUID",
                                "userAccountControl",
                                "accountExpires",
                                "pwdAccountLockedTime",
                                "shadowExpire",
                                "sambaAcctFlags",
                            ],
                            2,
                        ),
                        false,
                        ReferralPolicy::Reject,
                        2,
                    )
                    .await
                    .map_err(error)?;
                if rows.len() != 1 || !row::enabled(&rows[0], ad, self.clock.now().get())? {
                    return Err(incomplete());
                }
                let external = row::immutable(&rows[0], ad, &profile.identity_aliases)?;
                wire.bind(rows[0].dn.clone(), password.expose().to_owned())
                    .await
                    .map_err(error)?;
                VerifiedDirectorySubject::from_verified_bind(
                    connection,
                    external,
                    self.clock.now().get(),
                )
            }
            .await;
            wire.close().await;
            result
        })
    }
    fn observe<'a>(
        &'a self,
        connection: &'a DirectoryConnectionDescriptor,
        observed_at: i64,
    ) -> AccountFuture<'a, CompleteDirectorySnapshot> {
        Box::pin(async move {
            let lease = self
                .profiles
                .retain(
                    &connection.config.secret_profile,
                    &connection.connection_id,
                    Some(&connection.profile_fingerprint),
                )
                .await?;
            let profile = lease.profile();
            let ad = connection.provider == DirectoryProvider::ActiveDirectory;
            let mut wire = transport(connection, profile)?
                .connect()
                .await
                .map_err(error)?;
            let result = async {
                bind(&mut wire, profile).await?;
                let referrals = if ad {
                    ReferralPolicy::IgnoreActiveDirectoryPartition
                } else {
                    ReferralPolicy::Reject
                };
                let base = connection.config.base_dn.as_deref().unwrap_or("");
                let group_base = connection.config.group_base_dn.as_deref().unwrap_or(base);
                let users = wire
                    .search(
                        request(
                            base,
                            LdapSearchScope::Subtree,
                            user_filter(ad, connection.config.user_filter.as_deref())?,
                            &[
                                "objectGUID",
                                "entryUUID",
                                "uid",
                                "sAMAccountName",
                                "userPrincipalName",
                                "displayName",
                                "cn",
                                "mail",
                                "mailPrimaryAddress",
                                "userAccountControl",
                                "accountExpires",
                                "pwdAccountLockedTime",
                                "shadowExpire",
                                "sambaAcctFlags",
                            ],
                            10001,
                        ),
                        true,
                        referrals,
                        10000,
                    )
                    .await
                    .map_err(error)?;
                let filter = if ad {
                    LdapFilter::Equality("objectClass".into(), "group".into())
                } else {
                    LdapFilter::Or(
                        ["posixGroup", "univentionGroup", "groupOfUniqueNames"]
                            .map(|value| LdapFilter::Equality("objectClass".into(), value.into()))
                            .to_vec(),
                    )
                };
                let mut groups = wire
                    .search(
                        request(
                            group_base,
                            LdapSearchScope::Subtree,
                            filter,
                            &[
                                "objectGUID",
                                "entryUUID",
                                "cn",
                                "member",
                                "uniqueMember",
                                "memberUid",
                            ],
                            10001,
                        ),
                        true,
                        referrals,
                        10000,
                    )
                    .await
                    .map_err(error)?;
                for group in &mut groups {
                    complete_ranges(&mut wire, group).await?;
                }
                let container_filter = || {
                    LdapFilter::Or(
                        ["organizationalUnit", "container"]
                            .map(|value| LdapFilter::Equality("objectClass".into(), value.into()))
                            .to_vec(),
                    )
                };
                let attributes = ["objectGUID", "entryUUID", "ou", "cn", "objectClass"];
                let mut containers = wire
                    .search(
                        request(
                            base,
                            LdapSearchScope::Subtree,
                            container_filter(),
                            &attributes,
                            10001,
                        ),
                        true,
                        referrals,
                        10000,
                    )
                    .await
                    .map_err(error)?;
                let group_lower = group_base.to_lowercase();
                let base_lower = base.to_lowercase();
                if !group_base.is_empty()
                    && group_lower != base_lower
                    && !group_lower.ends_with(&format!(",{base_lower}"))
                {
                    let other = wire
                        .search(
                            request(
                                group_base,
                                LdapSearchScope::Subtree,
                                container_filter(),
                                &attributes,
                                10001,
                            ),
                            true,
                            referrals,
                            10000,
                        )
                        .await
                        .map_err(error)?;
                    let mut known = containers
                        .iter()
                        .map(|row| row.dn.to_lowercase())
                        .collect::<BTreeSet<_>>();
                    for row in other {
                        if known.insert(row.dn.to_lowercase()) {
                            containers.push(row);
                        }
                    }
                }
                row::snapshot(
                    users,
                    groups,
                    containers,
                    ad,
                    &profile.identity_aliases,
                    observed_at,
                    self.clock.now().get(),
                )
            }
            .await;
            wire.close().await;
            result
        })
    }
}
async fn complete_ranges<S: AsyncRead + AsyncWrite + Unpin>(
    wire: &mut SequentialLdap<S>,
    group: &mut LdapSearchResultEntry,
) -> Result<(), BusinessError> {
    let mut members = group
        .attributes
        .iter()
        .filter(|a| a.atype.eq_ignore_ascii_case("member"))
        .flat_map(|a| a.vals.clone())
        .collect::<Vec<_>>();
    let mut expected = 0usize;
    let mut page = None;
    for count in 0..100 {
        let current = page.as_ref().unwrap_or(group);
        let ranges = current
            .attributes
            .iter()
            .filter(|a| a.atype.to_ascii_lowercase().starts_with("member;range="))
            .collect::<Vec<_>>();
        if ranges.is_empty() {
            if expected > 0 {
                return Err(incomplete());
            }
            return Ok(());
        }
        if ranges.len() != 1 {
            return Err(incomplete());
        }
        let range = ranges[0];
        let name = range.atype.to_ascii_lowercase();
        let (start, end) = name
            .strip_prefix("member;range=")
            .and_then(|v| v.split_once('-'))
            .ok_or_else(incomplete)?;
        if start.is_empty()
            || !start.bytes().all(|v| v.is_ascii_digit())
            || start.parse::<usize>().map_err(|_| incomplete())? != expected
        {
            return Err(incomplete());
        }
        for value in &range.vals {
            if value.len() > 16_384 || members.len() >= MAX_DIRECTORY_EDGES {
                return Err(incomplete());
            }
            members.push(value.clone());
        }
        if end == "*" {
            group.attributes.retain(|a| {
                !a.atype.eq_ignore_ascii_case("member")
                    && !a.atype.to_ascii_lowercase().starts_with("member;range=")
            });
            group.attributes.push(LdapPartialAttribute {
                atype: "member".into(),
                vals: members,
            });
            return Ok(());
        }
        if end.is_empty() || !end.bytes().all(|v| v.is_ascii_digit()) || count == 99 {
            return Err(incomplete());
        }
        let end = end.parse::<usize>().map_err(|_| incomplete())?;
        if end < expected {
            return Err(incomplete());
        }
        expected = end.checked_add(1).ok_or_else(incomplete)?;
        let attribute = format!("member;range={expected}-*");
        let mut rows = wire
            .search(
                request(
                    &group.dn,
                    LdapSearchScope::Base,
                    LdapFilter::Present("objectClass".into()),
                    &[&attribute],
                    2,
                ),
                false,
                ReferralPolicy::Reject,
                1,
            )
            .await
            .map_err(error)?;
        if rows.len() != 1 || rows[0].dn.to_lowercase() != group.dn.to_lowercase() {
            return Err(incomplete());
        }
        page = rows.pop();
    }
    Err(incomplete())
}
