//! Authoritative workspace collection delivery through the federated Item Directory.
//!
//! Every request is re-authorized by the Reference service using the selected
//! session role and exact organization scope. The console supplies only a
//! cataloged record type; grants, counts, and payload visibility remain owned
//! by the server-side Item Directory.

use std::{collections::BTreeSet, str::FromStr};

use erpx_delivery_contract::{
    DeliveryError, Page, QueryRequest, ResourceName, ResourceSummary, RouteData, RouteQuery,
    RouteUuid, Workspace,
};
use erpx_integration_contracts::catalog::record_type;
use erpx_proto::erpx::{
    interface::{DirectoryFreshness, ResolveReferenceRequest, SearchItemsRequest},
    reference::reference_query_service_client::ReferenceQueryServiceClient,
};

use crate::feature_delivery::server;

pub(crate) async fn query_route(request: &QueryRequest) -> Result<RouteData, DeliveryError> {
    match &request.route {
        RouteQuery::Workspace {
            workspace,
            resource,
        } => list_workspace(*workspace, resource.as_ref(), request).await,
        RouteQuery::Record {
            workspace,
            resource,
            id,
        } => get_workspace_record(*workspace, resource, *id, request).await,
        RouteQuery::Reference { id } => get_reference(*id, request).await,
        _ => Err(DeliveryError::validation("route is not a workspace read")),
    }
}

async fn list_workspace(
    workspace: Workspace,
    resource: Option<&ResourceName>,
    request: &QueryRequest,
) -> Result<RouteData, DeliveryError> {
    let record_type = workspace_record_type(workspace, resource)
        .ok_or_else(|| DeliveryError::validation("workspace resource is not cataloged"))?;
    let authority = server::feature_authority()?;
    let scope = server::reference_scope(&authority)?;
    let wire = SearchItemsRequest {
        authority_scope: Some(scope),
        purpose: "authorized_workspace_collection".into(),
        query: String::new(),
        page_size: u32::from(request.limit.min(100)),
        cursor: request.cursor.clone(),
        record_type: Some(record_type.to_owned()),
    };
    let mut client = ReferenceQueryServiceClient::new(server::channel(&authority).await?);
    let response = match client
        .search(server::authenticated(&authority, wire)?)
        .await
    {
        Ok(response) => response.into_inner(),
        Err(status) => {
            tracing::warn!(
                grpc_code = ?status.code(),
                grpc_message = status.message(),
                workspace = %workspace,
                record_type,
                "authorized workspace collection query failed"
            );
            let error = server::delivery_status(status);
            if matches!(error, DeliveryError::Unavailable(_)) {
                return Ok(unavailable_route_data(
                    &request.route,
                    workspace.to_string(),
                    BTreeSet::from([
                        format!("workspace:{workspace}"),
                        format!("record-type:{record_type}"),
                    ]),
                ));
            }
            return Err(error);
        }
    };
    let freshness_tag = directory_freshness(response.freshness)?;
    let expected_prefix = format!("/{}/", route_module(workspace));
    let mut items = Vec::with_capacity(response.authorized_summaries.len());
    for summary in response.authorized_summaries {
        let record = summary
            .record
            .ok_or_else(|| DeliveryError::unavailable(Some(500)))?;
        if record.record_type != record_type
            || !summary.canonical_route.starts_with(&expected_prefix)
            || summary.display_label.trim().is_empty()
            || summary.descriptor_version == 0
        {
            return Err(DeliveryError::unavailable(Some(500)));
        }
        let id =
            RouteUuid::from_str(&record.id).map_err(|_| DeliveryError::unavailable(Some(500)))?;
        items.push(ResourceSummary {
            id,
            title: summary.display_label,
            status: summary.lifecycle_state,
            version: summary.descriptor_version,
        });
    }
    if items.len() > usize::from(request.limit) || response.authorized_count < items.len() as u64 {
        return Err(DeliveryError::unavailable(Some(500)));
    }
    Ok(RouteData {
        canonical_path: canonical_path(&request.route),
        title: workspace.to_string(),
        description: format!(
            "Authorized {} records from the federated Item Directory.",
            record_type
        ),
        page: Page {
            items,
            next_cursor: response.next_cursor,
        },
        freshness_tag,
        invalidation_tags: BTreeSet::from([
            format!("workspace:{workspace}"),
            format!("record-type:{record_type}"),
        ]),
        public_validator: None,
    })
}

async fn get_workspace_record(
    workspace: Workspace,
    resource: &ResourceName,
    id: RouteUuid,
    request: &QueryRequest,
) -> Result<RouteData, DeliveryError> {
    let expected_type = workspace_record_type(workspace, Some(resource))
        .ok_or_else(|| DeliveryError::validation("workspace resource is not cataloged"))?;
    let data = resolve(id, request, Some(expected_type)).await?;
    if !data
        .canonical_path
        .starts_with(&format!("/{}/", route_module(workspace)))
    {
        return Err(DeliveryError::unavailable(Some(500)));
    }
    Ok(data)
}

async fn get_reference(id: RouteUuid, request: &QueryRequest) -> Result<RouteData, DeliveryError> {
    resolve(id, request, None).await
}

async fn resolve(
    id: RouteUuid,
    request: &QueryRequest,
    expected_type: Option<&str>,
) -> Result<RouteData, DeliveryError> {
    let authority = server::feature_authority()?;
    let scope = server::reference_scope(&authority)?;
    let wire = ResolveReferenceRequest {
        record_id: id.to_string(),
        authority_scope: Some(scope),
        purpose: "authorized_workspace_record".into(),
        record_type: expected_type.map(str::to_owned),
    };
    let mut client = ReferenceQueryServiceClient::new(server::channel(&authority).await?);
    let response = match client
        .resolve(server::authenticated(&authority, wire)?)
        .await
    {
        Ok(response) => response.into_inner(),
        Err(status) => {
            tracing::warn!(
                grpc_code = ?status.code(),
                grpc_message = status.message(),
                record_id = %id,
                "authorized workspace record query failed"
            );
            let error = server::delivery_status(status);
            if matches!(error, DeliveryError::Unavailable(_)) {
                return Ok(unavailable_route_data(
                    &request.route,
                    expected_type.unwrap_or("Reference").to_owned(),
                    BTreeSet::from([format!("record:{id}")]),
                ));
            }
            return Err(error);
        }
    };
    let freshness_tag = directory_freshness(response.freshness)?;
    if !response.found {
        return Err(DeliveryError::NotFound(error_meta(false, None)));
    }
    let descriptor = response
        .descriptor
        .ok_or_else(|| DeliveryError::unavailable(Some(500)))?;
    let record = descriptor
        .record
        .ok_or_else(|| DeliveryError::unavailable(Some(500)))?;
    if record.id != id.to_string()
        || expected_type.is_some_and(|expected| record.record_type != expected)
        || descriptor.canonical_route.contains(['?', '#'])
        || !descriptor.canonical_route.starts_with('/')
        || descriptor.descriptor_version == 0
    {
        return Err(DeliveryError::unavailable(Some(500)));
    }
    let title = descriptor
        .display_label
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| id.to_string());
    Ok(RouteData {
        canonical_path: canonical_path(&request.route),
        title: title.clone(),
        description: format!("Authorized {} record.", record.record_type),
        page: Page {
            items: vec![ResourceSummary {
                id,
                title,
                status: descriptor.lifecycle_state,
                version: descriptor.descriptor_version,
            }],
            next_cursor: None,
        },
        freshness_tag,
        invalidation_tags: BTreeSet::from([
            format!("record:{}", id),
            format!("record-type:{}", record.record_type),
        ]),
        public_validator: None,
    })
}

fn directory_freshness(value: i32) -> Result<String, DeliveryError> {
    match DirectoryFreshness::try_from(value) {
        Ok(DirectoryFreshness::Fresh) => Ok("fresh".into()),
        Ok(DirectoryFreshness::Stale) => Ok("stale".into()),
        Ok(DirectoryFreshness::Unavailable | DirectoryFreshness::Unspecified) | Err(_) => {
            Err(DeliveryError::unavailable(Some(500)))
        }
    }
}

fn workspace_record_type(
    workspace: Workspace,
    resource: Option<&ResourceName>,
) -> Option<&'static str> {
    let record = resource.map(ResourceName::as_str).map_or_else(
        || default_record(workspace),
        |value| resource_alias(workspace, value),
    );
    let id = format!("{}.{}", catalog_module(workspace), record);
    record_type(&id).map(|descriptor| descriptor.id)
}

const fn catalog_module(workspace: Workspace) -> &'static str {
    match workspace {
        Workspace::Ai => "collaboration",
        _ => workspace.slug(),
    }
}

const fn route_module(workspace: Workspace) -> &'static str {
    match workspace {
        Workspace::Ai => "collaboration",
        _ => workspace.slug(),
    }
}

const fn default_record(workspace: Workspace) -> &'static str {
    match workspace {
        Workspace::Operations => "exception",
        Workspace::Finance => "journal",
        Workspace::Sales => "order",
        Workspace::Procurement => "purchase-order",
        Workspace::Inventory => "document",
        Workspace::Asset => "asset",
        Workspace::Maintenance => "work-order",
        Workspace::Quality => "inspection",
        Workspace::Marketing => "campaign",
        Workspace::People => "employee",
        Workspace::Pos => "sale",
        Workspace::Legal => "contract",
        Workspace::Manufacturing => "order",
        Workspace::Planning => "demand-plan",
        Workspace::Project => "project",
        Workspace::Commerce => "catalog-publication",
        Workspace::Talent => "candidate",
        Workspace::Learning => "enrollment",
        Workspace::Benefits => "plan",
        Workspace::FieldService => "visit",
        Workspace::Ai => "thread",
    }
}

fn resource_alias(workspace: Workspace, value: &str) -> &str {
    match (workspace, value) {
        (Workspace::Operations, "exceptions") => "exception",
        (Workspace::Finance, "journal-entries") => "journal",
        (Workspace::Sales, "orders") => "order",
        (Workspace::Procurement, "purchase-orders") => "purchase-order",
        (Workspace::Inventory, "stock-items") => "document",
        (Workspace::Asset, "assets") => "asset",
        (Workspace::Maintenance, "work-orders") => "work-order",
        (Workspace::Quality, "inspections") => "inspection",
        (Workspace::Marketing, "campaigns") => "campaign",
        (Workspace::People, "employees") => "employee",
        (Workspace::Pos, "sales") => "sale",
        (Workspace::Legal, "contracts") => "contract",
        (Workspace::Manufacturing, "production-orders") => "order",
        (Workspace::Planning, "demand-plans") => "demand-plan",
        (Workspace::Project, "projects") => "project",
        (Workspace::Commerce, "listings") => "catalog-publication",
        (Workspace::Talent, "candidates") => "candidate",
        (Workspace::Learning, "enrollments") => "enrollment",
        (Workspace::Benefits, "benefit-plans") => "plan",
        (Workspace::FieldService, "service-orders") => "visit",
        (Workspace::Ai, "conversations") => "thread",
        (_, value) => value,
    }
}

fn canonical_path(route: &RouteQuery) -> String {
    match route {
        RouteQuery::Workspace {
            workspace,
            resource,
        } => resource.as_ref().map_or_else(
            || format!("/{workspace}"),
            |resource| format!("/{workspace}/{resource}"),
        ),
        RouteQuery::Record {
            workspace,
            resource,
            id,
        } => format!("/{workspace}/{resource}/{id}"),
        RouteQuery::Reference { id } => format!("/reference/item/{id}"),
        _ => String::new(),
    }
}

fn unavailable_route_data(
    route: &RouteQuery,
    title: String,
    invalidation_tags: BTreeSet<String>,
) -> RouteData {
    RouteData {
        canonical_path: canonical_path(route),
        title,
        description: "The authoritative record projection is currently unavailable.".into(),
        page: Page {
            items: Vec::new(),
            next_cursor: None,
        },
        freshness_tag: "unavailable".into(),
        invalidation_tags,
        public_validator: None,
    }
}

fn error_meta(
    retryable: bool,
    retry_after_ms: Option<u64>,
) -> erpx_delivery_contract::DeliveryErrorMeta {
    erpx_delivery_contract::DeliveryErrorMeta {
        correlation_id: uuid::Uuid::new_v4(),
        retry_after_ms,
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_workspace_default_maps_to_a_catalog_record_type() {
        for workspace in Workspace::ALL {
            assert!(
                workspace_record_type(workspace, None).is_some(),
                "{workspace} has no authoritative default record type"
            );
        }
    }

    #[test]
    fn legacy_root_resources_map_without_accepting_unknown_families() {
        let finance: ResourceName = "journal-entries".parse().unwrap();
        assert_eq!(
            workspace_record_type(Workspace::Finance, Some(&finance)),
            Some("finance.journal")
        );
        let unknown: ResourceName = "secrets".parse().unwrap();
        assert_eq!(
            workspace_record_type(Workspace::Finance, Some(&unknown)),
            None
        );
    }

    #[test]
    fn unavailable_workspace_projection_exposes_no_payload_count_or_cursor() {
        let route = RouteQuery::Workspace {
            workspace: Workspace::Finance,
            resource: None,
        };
        let data = unavailable_route_data(
            &route,
            "Finance".into(),
            BTreeSet::from(["workspace:finance".into()]),
        );
        assert_eq!(data.canonical_path, "/finance");
        assert_eq!(data.freshness_tag, "unavailable");
        assert!(data.page.items.is_empty());
        assert!(data.page.next_cursor.is_none());
    }
}
