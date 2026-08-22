//! Server-only authorized navigation transport.

use erpx_proto::{
    common::{
        ActorAttribution, ActorKind, AuditContext, AuthorityScope, BranchScope, CompanyScope,
        EventualConsistency, HoldingScope, PlatformScope, QueryConsistency, QueryContext,
        SubtreeScope, authority_scope, query_consistency,
    },
    erpx::platform_administration as platform,
};
use erpx_ui_core::{NavAccess, NavGroup, NavItem};
use tonic::{Request, metadata::MetadataValue};
use uuid::Uuid;

pub(crate) async fn fetch_navigation_manifest(
    grpc_origin: &str,
    access_token: String,
    principal_ref: String,
    session_id: String,
    scope_key: String,
    development_context_token: Option<String>,
    include_inaccessible: bool,
) -> Result<Vec<NavGroup>, String> {
    let (active_scope, actor_kind) = authority_scope(&scope_key)?;
    let correlation_id = Uuid::new_v4().to_string();
    let context = QueryContext {
        contract_version: 1,
        request_id: Uuid::new_v4().to_string(),
        correlation_id: correlation_id.clone(),
        actor: Some(ActorAttribution {
            principal_ref: principal_ref.clone(),
            kind: actor_kind as i32,
            session_id: session_id.clone(),
            represented_principal_ref: String::new(),
            support_capability_id: String::new(),
        }),
        active_scope: Some(active_scope),
        consistency: Some(QueryConsistency {
            requirement: Some(query_consistency::Requirement::Eventual(
                EventualConsistency {},
            )),
        }),
        audit: Some(AuditContext {
            purpose: "authorized_navigation".into(),
            customer_visible: true,
            module: "shell".into(),
            correlation_id,
        }),
    };
    let mut request = Request::new(platform::GetNavigationManifestRequest {
        context: Some(context),
        include_inaccessible,
    });
    request
        .metadata_mut()
        .insert("x-erpx-access-token", metadata(&access_token)?);
    request
        .metadata_mut()
        .insert("x-erpx-actor", metadata(&principal_ref)?);
    request
        .metadata_mut()
        .insert("x-erpx-session", metadata(&session_id)?);
    if let Some(token) = development_context_token.as_deref() {
        request
            .metadata_mut()
            .insert("x-erpx-dev-persona-context", metadata(token)?);
    }
    let endpoint = tonic::transport::Endpoint::from_shared(grpc_origin.to_owned())
        .map_err(|_| "navigation transport unavailable: Unavailable".to_owned())?;
    let channel = endpoint
        .connect()
        .await
        .map_err(|_| "navigation transport unavailable: Unavailable".to_owned())?;
    let mut client = platform::platform_administration_query_service_client::PlatformAdministrationQueryServiceClient::new(channel);
    let response = client
        .get_navigation_manifest(request)
        .await
        .map_err(|status| format!("navigation manifest failed: {:?}", status.code()))?
        .into_inner();
    let manifest_has_authorized_item = response.groups.iter().any(|group| {
        group.items.iter().any(|item| {
            matches!(
                platform::NavigationAccessState::try_from(item.access_state),
                Ok(platform::NavigationAccessState::Authorized)
            )
        })
    });
    let groups = response
        .groups
        .into_iter()
        .filter_map(|group| {
            let items = group
                .items
                .into_iter()
                .filter_map(|item| {
                    if !crate::routes::is_canonical_console_path(
                        &item.path,
                        erpx_delivery_contract::ClientChannel::Web,
                    ) {
                        return None;
                    }
                    let access = match platform::NavigationAccessState::try_from(item.access_state)
                    {
                        Ok(platform::NavigationAccessState::Locked) => {
                            NavAccess::locked(item.required_role_keys)
                        }
                        Ok(platform::NavigationAccessState::Authorized) => NavAccess::Authorized,
                        Ok(platform::NavigationAccessState::Unspecified) | Err(_) => return None,
                    };
                    Some(
                        NavItem::new(item.label, item.path, item.icon)
                            .with_manifest_metadata(item.route_id, item.label_key)
                            .with_action_capabilities(item.action_capabilities)
                            .with_access(access),
                    )
                })
                .collect::<Vec<_>>();
            (!items.is_empty()).then(|| {
                NavGroup::new(group.label, items)
                    .with_manifest_metadata(group.group_id, group.label_key)
            })
        })
        .collect::<Vec<_>>();
    if groups.is_empty() && manifest_has_authorized_item {
        Ok(work_fallback_navigation())
    } else if groups.is_empty() {
        Err("authorized navigation manifest is empty".to_owned())
    } else {
        Ok(groups)
    }
}

fn work_fallback_navigation() -> Vec<NavGroup> {
    vec![
        NavGroup::new(
            "Work",
            vec![
                NavItem::new("Work", "/work", "work")
                    .with_manifest_metadata("work", "nav.route.work"),
            ],
        )
        .with_manifest_metadata("common", "nav.group.common"),
    ]
}

fn canonical_uuid(value: &str) -> Result<String, String> {
    Uuid::parse_str(value)
        .map(|value| value.to_string())
        .map_err(|_| "navigation scope invalid: InvalidArgument".to_owned())
}

pub(crate) fn authority_scope(scope_key: &str) -> Result<(AuthorityScope, ActorKind), String> {
    let invalid = || "navigation scope invalid: InvalidArgument".to_owned();
    let scope = if scope_key == "platform" {
        return Ok((
            AuthorityScope {
                scope: Some(authority_scope::Scope::Platform(PlatformScope {})),
            },
            ActorKind::PlatformUser,
        ));
    } else if let Some(value) = scope_key.strip_prefix("holding:") {
        authority_scope::Scope::Holding(HoldingScope {
            holding_id: canonical_uuid(value)?,
        })
    } else if let Some(value) = scope_key.strip_prefix("company:") {
        let (holding_id, company_id) = value.split_once(':').ok_or_else(invalid)?;
        authority_scope::Scope::Company(CompanyScope {
            holding_id: canonical_uuid(holding_id)?,
            company_id: canonical_uuid(company_id)?,
        })
    } else if let Some(value) = scope_key.strip_prefix("branch:") {
        let parts = value.split(':').collect::<Vec<_>>();
        let [holding_id, company_id, branch_id] = parts.as_slice() else {
            return Err(invalid());
        };
        authority_scope::Scope::Branch(BranchScope {
            holding_id: canonical_uuid(holding_id)?,
            company_id: canonical_uuid(company_id)?,
            branch_id: canonical_uuid(branch_id)?,
        })
    } else if let Some(value) = scope_key.strip_prefix("subtree:") {
        let (holding_id, organization_node_id) = value.split_once(':').ok_or_else(invalid)?;
        authority_scope::Scope::Subtree(SubtreeScope {
            holding_id: canonical_uuid(holding_id)?,
            organization_node_id: canonical_uuid(organization_node_id)?,
        })
    } else {
        return Err(invalid());
    };
    Ok((
        AuthorityScope { scope: Some(scope) },
        ActorKind::HoldingUser,
    ))
}

fn metadata(value: &str) -> Result<MetadataValue<tonic::metadata::Ascii>, String> {
    MetadataValue::try_from(value)
        .map_err(|_| "navigation credential invalid: Unauthenticated".to_owned())
}

#[cfg(test)]
mod tests {
    use super::work_fallback_navigation;

    #[test]
    fn authorized_unimplemented_navigation_falls_back_to_the_work_shell() {
        let groups = work_fallback_navigation();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].group_id, "common");
        assert_eq!(groups[0].items.len(), 1);
        assert_eq!(groups[0].items[0].path, "/work");
        assert!(groups[0].items[0].is_authorized());
    }
}
