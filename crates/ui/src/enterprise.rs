use erpx_integration_contracts::catalog::{ChannelId, WORKSPACES, WorkspaceId};

use crate::{Locale, NavGroup, NavItem, SafeUiText, normalize_deep_link};

pub const REFERENCE_ITEM_TEMPLATE: &str = "/reference/item/:id";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterpriseHubRoute {
    Work,
    Process,
    Insight,
    Module,
}

impl EnterpriseHubRoute {
    pub const fn path(self) -> &'static str {
        match self {
            Self::Work => "/work",
            Self::Process => "/processes",
            Self::Insight => "/insights",
            Self::Module => "/modules",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalAppRoute {
    Hub(EnterpriseHubRoute),
    Workspace {
        id: WorkspaceId,
        path: &'static str,
    },
    Collection {
        workspace: WorkspaceId,
        collection: String,
    },
    Detail {
        workspace: WorkspaceId,
        record: String,
        id: String,
    },
    ReferenceItem {
        id: String,
    },
}

impl CanonicalAppRoute {
    pub fn parse(input: &str) -> Result<Self, CanonicalRouteError> {
        let normalized = normalize_deep_link(input);
        if normalized == "/"
            || normalized.starts_with("/workspaces")
            || normalized.starts_with("/api/bff")
        {
            return Err(CanonicalRouteError::UnsupportedPath);
        }
        for hub in [
            EnterpriseHubRoute::Work,
            EnterpriseHubRoute::Process,
            EnterpriseHubRoute::Insight,
            EnterpriseHubRoute::Module,
        ] {
            if normalized == hub.path() {
                return Ok(Self::Hub(hub));
            }
        }
        let segments = normalized.trim_matches('/').split('/').collect::<Vec<_>>();
        if let ["reference", "item", id] = segments.as_slice() {
            return valid_segment(id)
                .then(|| Self::ReferenceItem { id: (*id).into() })
                .ok_or(CanonicalRouteError::InvalidIdentifier);
        }
        let Some(workspace) = WORKSPACES
            .iter()
            .find(|workspace| workspace.slug == segments[0])
        else {
            return Err(CanonicalRouteError::UnsupportedPath);
        };
        match segments.as_slice() {
            [_] => Ok(Self::Workspace {
                id: workspace.id,
                path: workspace.route,
            }),
            [_, collection] if valid_record_name(collection) => Ok(Self::Collection {
                workspace: workspace.id,
                collection: (*collection).into(),
            }),
            [_, record, id] if valid_record_name(record) && valid_segment(id) => Ok(Self::Detail {
                workspace: workspace.id,
                record: (*record).into(),
                id: (*id).into(),
            }),
            _ => Err(CanonicalRouteError::UnsupportedPath),
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::Hub(value) => value.path().into(),
            Self::Workspace { path, .. } => (*path).into(),
            Self::Collection {
                workspace,
                collection,
            } => format!("{}/{}", workspace_descriptor(*workspace).route, collection),
            Self::Detail {
                workspace,
                record,
                id,
            } => format!(
                "{}/{}/{}",
                workspace_descriptor(*workspace).route,
                record,
                id
            ),
            Self::ReferenceItem { id } => format!("/reference/item/{id}"),
        }
    }
}

fn workspace_descriptor(
    id: WorkspaceId,
) -> &'static erpx_integration_contracts::catalog::WorkspaceDescriptor {
    WORKSPACES
        .iter()
        .find(|workspace| workspace.id == id)
        .expect("workspace catalog is exhaustive")
}

fn valid_record_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_segment(value: &str) -> bool {
    valid_record_name(value)
        && !value.contains("token")
        && !value.contains("secret")
        && !value.contains("password")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalRouteError {
    UnsupportedPath,
    InvalidIdentifier,
}

pub const fn workspace_label(id: WorkspaceId, locale: Locale) -> &'static str {
    match (id, locale) {
        (WorkspaceId::Operations, Locale::En) => "Operations",
        (WorkspaceId::Operations, Locale::Th) => "ปฏิบัติการ",
        (WorkspaceId::Finance, Locale::En) => "Finance",
        (WorkspaceId::Finance, Locale::Th) => "การเงิน",
        (WorkspaceId::Sales, Locale::En) => "Sales",
        (WorkspaceId::Sales, Locale::Th) => "การขาย",
        (WorkspaceId::Procurement, Locale::En) => "Procurement",
        (WorkspaceId::Procurement, Locale::Th) => "จัดซื้อ",
        (WorkspaceId::Inventory, Locale::En) => "Inventory",
        (WorkspaceId::Inventory, Locale::Th) => "สินค้าคงคลัง",
        (WorkspaceId::Assets, Locale::En) => "Assets",
        (WorkspaceId::Assets, Locale::Th) => "สินทรัพย์",
        (WorkspaceId::Maintenance, Locale::En) => "Maintenance",
        (WorkspaceId::Maintenance, Locale::Th) => "บำรุงรักษา",
        (WorkspaceId::Quality, Locale::En) => "Quality",
        (WorkspaceId::Quality, Locale::Th) => "คุณภาพ",
        (WorkspaceId::MarketingCrm, Locale::En) => "Marketing & CRM",
        (WorkspaceId::MarketingCrm, Locale::Th) => "การตลาดและ CRM",
        (WorkspaceId::HrPayroll, Locale::En) => "People & Payroll",
        (WorkspaceId::HrPayroll, Locale::Th) => "บุคลากรและเงินเดือน",
        (WorkspaceId::PointOfSale, Locale::En) => "Point of Sale",
        (WorkspaceId::PointOfSale, Locale::Th) => "จุดขาย",
        (WorkspaceId::Legal, Locale::En) => "Legal",
        (WorkspaceId::Legal, Locale::Th) => "กฎหมาย",
        (WorkspaceId::Manufacturing, Locale::En) => "Manufacturing",
        (WorkspaceId::Manufacturing, Locale::Th) => "การผลิต",
        (WorkspaceId::Planning, Locale::En) => "Planning",
        (WorkspaceId::Planning, Locale::Th) => "การวางแผน",
        (WorkspaceId::Projects, Locale::En) => "Projects",
        (WorkspaceId::Projects, Locale::Th) => "โครงการ",
        (WorkspaceId::Ecommerce, Locale::En) => "E-commerce",
        (WorkspaceId::Ecommerce, Locale::Th) => "อีคอมเมิร์ซ",
        (WorkspaceId::RecruitingTalent, Locale::En) => "Recruiting & Talent",
        (WorkspaceId::RecruitingTalent, Locale::Th) => "สรรหาและผู้มีศักยภาพ",
        (WorkspaceId::Learning, Locale::En) => "Learning",
        (WorkspaceId::Learning, Locale::Th) => "การเรียนรู้",
        (WorkspaceId::Benefits, Locale::En) => "Benefits",
        (WorkspaceId::Benefits, Locale::Th) => "สวัสดิการ",
        (WorkspaceId::FieldService, Locale::En) => "Field Service",
        (WorkspaceId::FieldService, Locale::Th) => "บริการภาคสนาม",
        (WorkspaceId::AiCollaboration, Locale::En) => "AI & Collaboration",
        (WorkspaceId::AiCollaboration, Locale::Th) => "AI และการทำงานร่วมกัน",
    }
}

pub fn canonical_workspace_navigation(locale: Locale) -> NavGroup {
    let items = WORKSPACES
        .iter()
        .map(|workspace| {
            NavItem::new(
                workspace_label(workspace.id, locale),
                workspace.route,
                "module",
            )
            .with_manifest_metadata(workspace.slug, format!("nav.workspace.{}", workspace.slug))
        })
        .collect();
    NavGroup::new(
        match locale {
            Locale::En => "Workspaces",
            Locale::Th => "พื้นที่ทำงาน",
        },
        items,
    )
    .with_manifest_metadata("workspaces", "nav.group.workspaces")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientFormFactor {
    Wide,
    Compact,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptiveDensity {
    Dense,
    Comfortable,
}
impl AdaptiveDensity {
    pub const fn for_form_factor(value: ClientFormFactor) -> Self {
        match value {
            ClientFormFactor::Wide => Self::Dense,
            ClientFormFactor::Compact => Self::Comfortable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationMode {
    Enabled,
    ReadOnlyOffline,
    Denied,
}
impl MutationMode {
    pub const fn may_submit(self) -> bool {
        matches!(self, Self::Enabled)
    }
    pub const fn queues_offline(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterpriseStateKind {
    Loading,
    Empty,
    Denied,
    Restricted,
    Stale,
    Degraded,
    Conflict,
    Retry,
    Success,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnterpriseState {
    pub kind: EnterpriseStateKind,
    pub title: String,
    pub detail: String,
    pub retryable: bool,
}
impl EnterpriseState {
    pub fn bounded(
        kind: EnterpriseStateKind,
        title: impl AsRef<str>,
        detail: impl AsRef<str>,
        retryable: bool,
    ) -> Self {
        Self {
            kind,
            title: SafeUiText::new(title).as_str().into(),
            detail: SafeUiText::new(detail).as_str().into(),
            retryable,
        }
    }
    pub const fn may_show_payload(&self) -> bool {
        matches!(
            self.kind,
            EnterpriseStateKind::Stale
                | EnterpriseStateKind::Degraded
                | EnterpriseStateKind::Conflict
                | EnterpriseStateKind::Success
        )
    }
}

pub fn workspace_supports_channel(id: WorkspaceId, channel: ChannelId) -> bool {
    workspace_descriptor(id).channels.contains(channel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_is_derived_from_the_exact_typed_catalog() {
        let group = canonical_workspace_navigation(Locale::En);
        assert_eq!(group.items.len(), 21);
        assert!(
            group
                .items
                .iter()
                .all(|item| !item.path.starts_with("/workspaces"))
        );
        assert_eq!(
            group.items.first().map(|item| item.path.as_str()),
            Some("/operations")
        );
        assert_eq!(
            group.items.last().map(|item| item.path.as_str()),
            Some("/ai")
        );
    }

    #[test]
    fn canonical_parser_rejects_legacy_and_supports_detail_and_reference_routes() {
        assert_eq!(
            CanonicalAppRoute::parse("/workspaces/finance"),
            Err(CanonicalRouteError::UnsupportedPath)
        );
        assert_eq!(
            CanonicalAppRoute::parse("/api/bff/v1/navigation"),
            Err(CanonicalRouteError::UnsupportedPath)
        );
        assert_eq!(
            CanonicalAppRoute::parse("/finance/journal/opaque-1")
                .unwrap()
                .path(),
            "/finance/journal/opaque-1"
        );
        assert_eq!(
            CanonicalAppRoute::parse("/reference/item/ref-1")
                .unwrap()
                .path(),
            "/reference/item/ref-1"
        );
    }

    #[test]
    fn offline_is_read_only_and_never_queues_mutations() {
        assert!(!MutationMode::ReadOnlyOffline.may_submit());
        assert!(!MutationMode::ReadOnlyOffline.queues_offline());
        assert_eq!(
            AdaptiveDensity::for_form_factor(ClientFormFactor::Wide),
            AdaptiveDensity::Dense
        );
        assert_eq!(
            AdaptiveDensity::for_form_factor(ClientFormFactor::Compact),
            AdaptiveDensity::Comfortable
        );
    }

    #[test]
    fn channel_policy_keeps_commerce_and_talent_web_only() {
        for workspace in WORKSPACES {
            assert!(workspace_supports_channel(workspace.id, ChannelId::Web));
            let is_web_only = matches!(
                workspace.id,
                WorkspaceId::Ecommerce | WorkspaceId::RecruitingTalent
            );
            for channel in [ChannelId::Desktop, ChannelId::Ios, ChannelId::Android] {
                assert_eq!(
                    workspace_supports_channel(workspace.id, channel),
                    !is_web_only
                );
            }
        }
    }

    #[test]
    fn restricted_and_denied_states_cannot_retain_payload() {
        for kind in [EnterpriseStateKind::Denied, EnterpriseStateKind::Restricted] {
            let state = EnterpriseState::bounded(kind, "Restricted", "token=secret", false);
            assert!(!state.may_show_payload());
            assert_eq!(state.detail, "[redacted]");
        }
    }
}
