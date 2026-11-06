use erpx_server_shared::public::BusinessError;
#[derive(Debug, PartialEq, Eq)]
pub struct WorkspacePreferenceScope {
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
}
fn invalid() -> BusinessError {
    BusinessError::invalid("INVALID_WORKSPACE", "Choose an available workspace.")
}
fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Zod `string.max` counts Unicode scalar values (code points), not UTF-8 bytes.
/// The gateway bound mirrors `workspaceKey.max(4096)` in account-switcher.
fn cp_len(value: &str) -> usize {
    value.chars().count()
}
/// Navigation protocol only. This returns untrusted selectors; Identity must
/// resolve their current lineage and the captured role before saving anything.
pub fn workspace_preference_scope(
    path: &str,
    holding_slug: &str,
) -> Result<WorkspacePreferenceScope, BusinessError> {
    // Slugs are ASCII-gated, so byte length equals code-point length there. The
    // stored path bound mirrors the gateway's code-point length check.
    if !slug(holding_slug) || cp_len(path) > 4096 {
        return Err(invalid());
    }
    let prefix = format!("/holdings/{holding_slug}");
    let suffix = path.strip_prefix(&prefix).ok_or_else(invalid)?;
    match suffix.split('/').collect::<Vec<_>>().as_slice() {
        ["", "home"] => Ok(WorkspacePreferenceScope {
            branch_slug: None,
            department_slug: None,
        }),
        ["", "branches", b, "home"] if slug(b) => Ok(WorkspacePreferenceScope {
            branch_slug: Some((*b).into()),
            department_slug: None,
        }),
        ["", "departments", d, "home"] if slug(d) => Ok(WorkspacePreferenceScope {
            branch_slug: None,
            department_slug: Some((*d).into()),
        }),
        ["", "branches", b, "departments", d, "home"] if slug(b) && slug(d) => {
            Ok(WorkspacePreferenceScope {
                branch_slug: Some((*b).into()),
                department_slug: Some((*d).into()),
            })
        }
        _ => Err(invalid()),
    }
}
