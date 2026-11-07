//! Complete LDAP observations become portable facts only after every required
//! page/range/search succeeds. This transformation never opens a database.
use crate::domain::{directory::*, directory_provider_values::*};
use erpx_server_shared::public::BusinessError;
use ldap3_proto::proto::LdapSearchResultEntry;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn values<'a>(
    row: &'a LdapSearchResultEntry,
    name: &str,
) -> Result<Vec<&'a str>, BusinessError> {
    let attrs = row
        .attributes
        .iter()
        .filter(|a| a.atype.eq_ignore_ascii_case(name))
        .collect::<Vec<_>>();
    if attrs.len() > 1 {
        return Err(incomplete());
    }
    attrs.first().map_or_else(
        || Ok(vec![]),
        |a| {
            a.vals
                .iter()
                .map(|v| std::str::from_utf8(v).map_err(|_| incomplete()))
                .collect()
        },
    )
}
pub(super) fn first<'a>(
    row: &'a LdapSearchResultEntry,
    name: &str,
) -> Result<&'a str, BusinessError> {
    Ok(values(row, name)?.first().copied().unwrap_or(""))
}
pub(super) fn immutable(
    row: &LdapSearchResultEntry,
    ad: bool,
    aliases: &BTreeMap<String, String>,
) -> Result<String, BusinessError> {
    let id = if ad {
        let attrs = row
            .attributes
            .iter()
            .filter(|a| a.atype.eq_ignore_ascii_case("objectGUID"))
            .collect::<Vec<_>>();
        if attrs.len() != 1 {
            return Err(incomplete());
        }
        ad_guid(attrs[0].vals.first().ok_or_else(incomplete)?).map_err(|_| incomplete())?
    } else {
        let id = first(row, "entryUUID")?;
        if id.is_empty() {
            return Err(incomplete());
        }
        id.to_owned()
    };
    Ok(canonical_external_id(aliases, &id))
}
pub(super) fn enabled(
    row: &LdapSearchResultEntry,
    ad: bool,
    now: i64,
) -> Result<bool, BusinessError> {
    LdapAccountControls {
        user_account_control: first(row, "userAccountControl")?,
        account_expires: first(row, "accountExpires")?,
        shadow_expire: first(row, "shadowExpire")?,
        password_locked_at: first(row, "pwdAccountLockedTime")?,
        samba_account_flags: first(row, "sambaAcctFlags")?,
    }
    .enabled(ad, now)
    .map_err(|_| incomplete())
}
fn retain(
    row: &LdapSearchResultEntry,
    budget: &mut ObservationBudget,
) -> Result<(), BusinessError> {
    budget.text(&row.dn)?;
    if row.attributes.len() > 128 {
        return Err(incomplete());
    }
    for a in &row.attributes {
        budget.text(&a.atype)?;
        for v in &a.vals {
            budget.edge()?;
            if v.len() > 16_384 {
                return Err(incomplete());
            }
            if let Ok(text) = std::str::from_utf8(v) {
                budget.text(text)?;
            } else if !a.atype.eq_ignore_ascii_case("objectGUID") {
                return Err(incomplete());
            }
        }
    }
    Ok(())
}
struct Group {
    external_id: String,
    name: String,
    dn: String,
    members: BTreeSet<String>,
    uids: BTreeSet<String>,
}

pub(super) fn snapshot(
    users: Vec<LdapSearchResultEntry>,
    groups: Vec<LdapSearchResultEntry>,
    containers: Vec<LdapSearchResultEntry>,
    ad: bool,
    aliases: &BTreeMap<String, String>,
    observed_at: i64,
    evaluated_at: i64,
) -> Result<CompleteDirectorySnapshot, BusinessError> {
    let mut budget = ObservationBudget::new();
    for row in users.iter().chain(groups.iter()).chain(containers.iter()) {
        retain(row, &mut budget)?;
    }
    let mut group_rows = Vec::with_capacity(groups.len());
    for group in &groups {
        let members = values(group, "member")?
            .into_iter()
            .chain(values(group, "uniqueMember")?)
            .map(str::to_lowercase)
            .collect();
        group_rows.push(Group {
            external_id: immutable(group, ad, aliases)?,
            name: first(group, "cn")?.to_owned(),
            dn: group.dn.to_lowercase(),
            members,
            uids: values(group, "memberUid")?
                .into_iter()
                .map(str::to_owned)
                .collect(),
        });
    }
    // Index facts once. Sorting numeric row positions preserves the original
    // Node group scan order without users*groups*depth work on a large snapshot.
    let mut parents_by_member: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut parents_by_uid: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut groups_by_dn: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, group) in group_rows.iter().enumerate() {
        groups_by_dn.entry(&group.dn).or_default().push(index);
        for member in &group.members {
            parents_by_member.entry(member).or_default().push(index);
        }
        for uid in &group.uids {
            parents_by_uid.entry(uid).or_default().push(index);
        }
    }
    let mut mapped = Vec::with_capacity(users.len());
    for user in &users {
        let username = first(user, if ad { "sAMAccountName" } else { "uid" })?;
        let mut seen = BTreeSet::new();
        let mut memberships = Vec::new();
        let mut frontier = BTreeSet::from([user.dn.to_lowercase()]);
        for depth in 0..64 {
            if frontier.is_empty() {
                break;
            }
            let mut next = BTreeSet::new();
            let mut candidates = BTreeSet::new();
            for member in &frontier {
                if let Some(indices) = parents_by_member.get(member.as_str()) {
                    candidates.extend(indices.iter().copied());
                }
            }
            if depth == 0
                && let Some(indices) = parents_by_uid.get(username)
            {
                candidates.extend(indices.iter().copied());
            }
            for index in candidates {
                let group = &group_rows[index];
                if !seen.contains(group.external_id.as_str()) {
                    budget.edge()?;
                    budget.text(&group.external_id)?;
                    seen.insert(group.external_id.as_str());
                    memberships.push(group.external_id.clone());
                    next.insert(group.dn.clone());
                }
            }
            frontier = next;
            if depth == 63 && !frontier.is_empty() {
                return Err(rejected(
                    "DIRECTORY_GROUP_DEPTH",
                    "Group nesting exceeds the supported depth.",
                ));
            }
        }
        let name = first(user, "displayName")?;
        let cn = first(user, "cn")?;
        let primary = first(user, "mailPrimaryAddress")?;
        let mail = first(user, "mail")?;
        mapped.push(ObservedDirectoryUser {
            external_id: immutable(user, ad, aliases)?,
            username: username.to_owned(),
            name: if !name.is_empty() {
                name
            } else if !cn.is_empty() {
                cn
            } else {
                username
            }
            .to_owned(),
            email: if !primary.is_empty() {
                Some(primary.to_owned())
            } else if !mail.is_empty() {
                Some(mail.to_owned())
            } else {
                None
            },
            enabled: enabled(user, ad, evaluated_at)?,
            group_ids: memberships,
        });
    }
    let mut container_rows = Vec::with_capacity(containers.len());
    let mut by_dn = BTreeMap::new();
    for row in &containers {
        let id = immutable(row, ad, aliases)?;
        let dn = row.dn.to_lowercase();
        by_dn.entry(dn.clone()).or_insert_with(|| id.clone());
        container_rows.push((id, dn, row));
    }
    let parent = |dn: &str| -> Option<String> {
        let mut escaped = false;
        for (index, ch) in dn.char_indices() {
            if !escaped && ch == ',' {
                let parent = dn[index + 1..]
                    .trim_start_matches(crate::domain::directory_provider_values::js_space)
                    .to_lowercase();
                return by_dn.get(&parent).cloned();
            }
            escaped = ch == '\\' && !escaped;
        }
        None
    };
    let mut hierarchy = DirectoryHierarchy {
        containers: vec![],
        placements: vec![],
        group_edges: vec![],
    };
    for (id, dn, row) in &container_rows {
        let ou = first(row, "ou")?;
        hierarchy.containers.push(ObservedContainer {
            external_id: id.clone(),
            parent_id: parent(dn),
            name: if ou.is_empty() { first(row, "cn")? } else { ou }.to_owned(),
            kind: if values(row, "objectClass")?
                .iter()
                .any(|v| v.to_lowercase() == "organizationalunit")
            {
                "ou"
            } else {
                "container"
            }
            .into(),
        });
    }
    for user in &users {
        hierarchy.placements.push(DirectoryPlacement {
            kind: "identity".into(),
            external_id: immutable(user, ad, aliases)?,
            container_id: parent(&user.dn),
        });
    }
    for group in &group_rows {
        hierarchy.placements.push(DirectoryPlacement {
            kind: "group".into(),
            external_id: group.external_id.clone(),
            container_id: parent(&group.dn),
        });
    }
    for group in &group_rows {
        let mut children = BTreeSet::new();
        for dn in &group.members {
            if let Some(indices) = groups_by_dn.get(dn.as_str()) {
                children.extend(indices.iter().copied());
            }
        }
        for index in children {
            budget.edge()?;
            hierarchy.group_edges.push(DirectoryGroupEdge {
                parent_id: group.external_id.clone(),
                child_id: group_rows[index].external_id.clone(),
            });
        }
    }
    CompleteDirectorySnapshot::checked(
        mapped,
        group_rows
            .into_iter()
            .map(|g| ObservedDirectoryGroup {
                external_id: g.external_id,
                name: g.name,
            })
            .collect(),
        hierarchy,
        observed_at,
    )
}
