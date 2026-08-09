use crate::{Locale, redact_deep_link};

/// Server-evaluated route access used only for navigation discovery. A locked
/// item remains visible, but every query and command still reauthorizes at its
/// authoritative boundary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NavAccess {
    /// The active role and scope may open this route.
    #[default]
    Authorized,
    /// The route is discoverable but cannot be opened with the active role.
    Locked { required_role_keys: Vec<String> },
}

impl NavAccess {
    pub fn locked(required_role_keys: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut required_role_keys = required_role_keys
            .into_iter()
            .map(Into::into)
            .filter(|value| valid_navigation_key(value, 256))
            .take(96)
            .collect::<Vec<_>>();
        required_role_keys.sort();
        required_role_keys.dedup();
        Self::Locked { required_role_keys }
    }

    pub const fn is_authorized(&self) -> bool {
        matches!(self, Self::Authorized)
    }

    pub fn required_role_keys(&self) -> &[String] {
        match self {
            Self::Authorized => &[],
            Self::Locked { required_role_keys } => required_role_keys,
        }
    }
}

/// A route-agnostic navigation entry. Applications may render `path` with Dioxus Router, a
/// native deep-link handler, or a normal anchor without changing the shared shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavItem {
    /// Stable route identifier supplied by the authorized manifest.
    pub route_id: String,
    /// Human-readable label, already localized by the application.
    pub label: String,
    /// Stable localization key; `label` remains the compatibility fallback.
    pub label_key: String,
    /// Stable application path or deep-link target.
    pub path: String,
    /// Short icon token interpreted by the client theme.
    pub icon: String,
    /// Server-owned workflow/action discoverability metadata. This can hide or
    /// disable controls, but it is never treated as command authorization.
    pub action_capabilities: Vec<String>,
    /// Server-evaluated navigation access and canonical role requirements.
    pub access: NavAccess,
}

impl NavItem {
    /// Create a navigation entry after removing query/fragment material and rejecting
    /// credential-bearing deep links.
    pub fn new(label: impl Into<String>, path: impl Into<String>, icon: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            route_id: String::new(),
            label: label.into(),
            label_key: String::new(),
            path: redact_deep_link(&path),
            icon: icon.into(),
            action_capabilities: Vec::new(),
            access: NavAccess::Authorized,
        }
    }

    /// Attach bounded, non-secret manifest identity metadata.
    pub fn with_manifest_metadata(
        mut self,
        route_id: impl Into<String>,
        label_key: impl Into<String>,
    ) -> Self {
        let route_id = route_id.into();
        let label_key = label_key.into();
        if valid_navigation_key(&route_id, 96) {
            self.route_id = route_id;
        }
        if valid_navigation_key(&label_key, 128) {
            self.label_key = label_key;
        }
        self
    }

    /// Attach bounded capability identifiers supplied by the navigation
    /// manifest. Invalid or credential-looking values are dropped before they
    /// can reach rendered attributes or diagnostic output.
    pub fn with_action_capabilities(
        mut self,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.action_capabilities = capabilities
            .into_iter()
            .map(Into::into)
            .filter(|value| valid_action_capability(value))
            .take(64)
            .collect();
        self.action_capabilities.sort();
        self.action_capabilities.dedup();
        self
    }

    /// Attach server-evaluated discovery access. Callers cannot manufacture
    /// command authorization with this metadata.
    pub fn with_access(mut self, access: NavAccess) -> Self {
        self.access = access;
        self
    }

    pub const fn is_authorized(&self) -> bool {
        self.access.is_authorized()
    }

    pub fn required_role_keys(&self) -> &[String] {
        self.access.required_role_keys()
    }

    /// Whether the manifest advertised a workflow action for this route.
    pub fn allows_action(&self, capability: &str) -> bool {
        valid_action_capability(capability)
            && self
                .action_capabilities
                .binary_search_by(|candidate| candidate.as_str().cmp(capability))
                .is_ok()
    }
}

fn valid_action_capability(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
        && !value.to_ascii_lowercase().contains("token")
        && !value.to_ascii_lowercase().contains("secret")
        && !value.to_ascii_lowercase().contains("password")
}

fn valid_navigation_key(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        && !value.contains("token")
        && !value.contains("secret")
        && !value.contains("password")
}

/// A named primary-navigation work area. Grouping is presentation metadata;
/// every child route remains independently authorized by the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavGroup {
    pub group_id: String,
    pub label: String,
    pub label_key: String,
    pub items: Vec<NavItem>,
}

impl NavGroup {
    pub fn new(label: impl Into<String>, items: Vec<NavItem>) -> Self {
        Self {
            group_id: String::new(),
            label: label.into(),
            label_key: String::new(),
            items,
        }
    }

    pub fn with_manifest_metadata(
        mut self,
        group_id: impl Into<String>,
        label_key: impl Into<String>,
    ) -> Self {
        let group_id = group_id.into();
        let label_key = label_key.into();
        if valid_navigation_key(&group_id, 96) {
            self.group_id = group_id;
        }
        if valid_navigation_key(&label_key, 128) {
            self.label_key = label_key;
        }
        self
    }
}

/// Resolve server-owned navigation localization keys while retaining the
/// compatibility label for older or independently deployed servers.
pub fn localized_navigation_label(key: &str, fallback: &str, locale: Locale) -> String {
    if let Some(route) = erpx_domain::data_plane::BUSINESS_UNIT_ROUTE_CATALOG
        .iter()
        .find(|route| route.label_key == key)
    {
        return match locale {
            Locale::En => route.label_en,
            Locale::Th => route.label_th,
        }
        .to_owned();
    }
    let translated = match (key, locale) {
        ("nav.group.customer_operations", Locale::En) => "Customer Operations",
        ("nav.group.customer_operations", Locale::Th) => "การดำเนินงานลูกค้า",
        ("nav.group.tenant_administration", Locale::En) => "Tenant Administration",
        ("nav.group.tenant_administration", Locale::Th) => "การดูแลผู้เช่า",
        ("nav.group.business_modules", Locale::En) => "Business Modules",
        ("nav.group.business_modules", Locale::Th) => "โมดูลธุรกิจ",
        ("nav.group.platform_administration", Locale::En) => "Platform Administration",
        ("nav.group.platform_administration", Locale::Th) => "การดูแลแพลตฟอร์ม",
        ("nav.route.customer_360", Locale::En) => "Customer 360",
        ("nav.route.customer_360", Locale::Th) => "ลูกค้า 360",
        ("nav.route.directories", Locale::En) => "Directories",
        ("nav.route.directories", Locale::Th) => "ไดเรกทอรี",
        ("nav.route.support", Locale::En) => "Support",
        ("nav.route.support", Locale::Th) => "ฝ่ายสนับสนุน",
        ("nav.route.billing", Locale::En) => "Billing",
        ("nav.route.billing", Locale::Th) => "การเรียกเก็บเงิน",
        ("nav.route.tenant_organization", Locale::En) => "Organization",
        ("nav.route.tenant_organization", Locale::Th) => "องค์กร",
        ("nav.route.tenant_branches", Locale::En) => "Branches",
        ("nav.route.tenant_branches", Locale::Th) => "สาขา",
        ("nav.route.tenant_identity_access", Locale::En) => "Identity & Access",
        ("nav.route.tenant_identity_access", Locale::Th) => "ตัวตนและสิทธิ์เข้าถึง",
        ("nav.route.tenant_infrastructure", Locale::En) => "Infrastructure",
        ("nav.route.tenant_infrastructure", Locale::Th) => "โครงสร้างพื้นฐาน",
        ("nav.route.tenant_configuration", Locale::En) => "Configuration",
        ("nav.route.tenant_configuration", Locale::Th) => "การกำหนดค่า",
        ("nav.route.tenant_root_access", Locale::En) => "Root Access",
        ("nav.route.tenant_root_access", Locale::Th) => "สิทธิ์ Root",
        ("nav.route.tenant_corporate_identity", Locale::En) => "Corporate Identity",
        ("nav.route.tenant_corporate_identity", Locale::Th) => "อัตลักษณ์องค์กร",
        ("nav.route.tenants", Locale::En) => "Tenants",
        ("nav.route.tenants", Locale::Th) => "ผู้เช่า",
        ("nav.route.projections", Locale::En) => "Projections",
        ("nav.route.projections", Locale::Th) => "โพรเจกชัน",
        ("nav.route.backups", Locale::En) => "Backups",
        ("nav.route.backups", Locale::Th) => "ข้อมูลสำรอง",
        ("nav.route.integrations", Locale::En) => "Integrations",
        ("nav.route.integrations", Locale::Th) => "การเชื่อมต่อ",
        ("nav.route.audit", Locale::En) => "Audit",
        ("nav.route.audit", Locale::Th) => "การตรวจสอบย้อนหลัง",
        ("nav.route.role_review", Locale::En) => "Role Review",
        ("nav.route.role_review", Locale::Th) => "ทบทวนบทบาท",
        ("nav.route.settings", Locale::En) => "Settings",
        ("nav.route.settings", Locale::Th) => "การตั้งค่า",
        _ => fallback,
    };
    translated.to_owned()
}

/// Localize a canonical role key without exposing permission keys or custom
/// local assignments. Business-unit role labels are derived from the shared
/// route catalog so the complete canonical matrix remains bilingual.
pub fn localized_role_label(role_key: &str, locale: Locale) -> String {
    let fixed = match (role_key, locale) {
        ("platform_customer_operations", Locale::En) => "Platform Customer Operations",
        ("platform_customer_operations", Locale::Th) => "ฝ่ายปฏิบัติการลูกค้าแพลตฟอร์ม",
        ("platform_directory_administrator", Locale::En) => "Platform Directory Administrator",
        ("platform_directory_administrator", Locale::Th) => "ผู้ดูแลไดเรกทอรีแพลตฟอร์ม",
        ("platform_support_agent", Locale::En) => "Platform Support Agent",
        ("platform_support_agent", Locale::Th) => "เจ้าหน้าที่สนับสนุนแพลตฟอร์ม",
        ("platform_billing_administrator", Locale::En) => "Platform Billing Administrator",
        ("platform_billing_administrator", Locale::Th) => "ผู้ดูแลการเรียกเก็บเงินแพลตฟอร์ม",
        ("platform_operations_administrator", Locale::En) => "Platform Operations Administrator",
        ("platform_operations_administrator", Locale::Th) => "ผู้ดูแลปฏิบัติการแพลตฟอร์ม",
        ("platform_auditor", Locale::En) => "Platform Auditor",
        ("platform_auditor", Locale::Th) => "ผู้ตรวจสอบแพลตฟอร์ม",
        ("platform_role_reviewer", Locale::En) => "Platform Role Reviewer",
        ("platform_role_reviewer", Locale::Th) => "ผู้ทบทวนบทบาทแพลตฟอร์ม",
        ("platform_tenant_administrator", Locale::En) => "Platform Tenant Administrator",
        ("platform_tenant_administrator", Locale::Th) => "ผู้ดูแล Tenant ของแพลตฟอร์ม",
        ("holding_owner", Locale::En) => "Holding Owner",
        ("holding_owner", Locale::Th) => "เจ้าของโฮลดิ้ง",
        ("holding_admin", Locale::En) => "Holding Administrator",
        ("holding_admin", Locale::Th) => "ผู้ดูแลโฮลดิ้ง",
        ("company_admin", Locale::En) => "Company Administrator",
        ("company_admin", Locale::Th) => "ผู้ดูแลบริษัท",
        ("branch_admin", Locale::En) => "Branch Administrator",
        ("branch_admin", Locale::Th) => "ผู้ดูแลสาขา",
        ("customer_security_head", Locale::En) => "Customer Security Head",
        ("customer_security_head", Locale::Th) => "หัวหน้าความปลอดภัยลูกค้า",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_owned();
    }

    for (suffix, en_level, th_level) in [
        ("_viewer", "Viewer", "ผู้ดูข้อมูล"),
        ("_operator", "Operator", "เจ้าหน้าที่"),
        ("_approver", "Approver", "ผู้อนุมัติ"),
        ("_administrator", "Administrator", "ผู้ดูแลระบบ"),
        ("_auditor", "Auditor", "ผู้ตรวจสอบ"),
    ] {
        let Some(unit) = role_key.strip_suffix(suffix) else {
            continue;
        };
        if let Some(route) = erpx_domain::data_plane::BUSINESS_UNIT_ROUTE_CATALOG
            .iter()
            .find(|route| route.route_id == unit)
        {
            let unit_label = match locale {
                Locale::En => route.label_en,
                Locale::Th => route.label_th,
            };
            return match locale {
                Locale::En => format!("{unit_label} {en_level}"),
                Locale::Th => format!("{th_level} {unit_label}"),
            };
        }
    }

    role_key
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(characters).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn localized_navigation_requirement(role_keys: &[String], locale: Locale) -> String {
    if role_keys.is_empty() {
        return match locale {
            Locale::En => {
                "No standard Role/Persona grants this route. Ask an administrator to configure access."
            }
            Locale::Th => {
                "ไม่มี Role/Persona มาตรฐานสำหรับหน้านี้ โปรดขอให้ผู้ดูแลกำหนดสิทธิ์เข้าถึง"
            }
        }
        .to_owned();
    }
    let labels = role_keys
        .iter()
        .map(|key| localized_role_label(key, locale))
        .collect::<Vec<_>>()
        .join(", ");
    match locale {
        Locale::En => format!("Requires one of these Roles/Personas: {labels}"),
        Locale::Th => format!("ต้องใช้ Role/Persona อย่างใดอย่างหนึ่ง: {labels}"),
    }
}

/// Minimal navigation state shared by responsive shells and mobile navigation drawers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NavigationState {
    active_path: String,
}

impl NavigationState {
    /// Create state for an initial path.
    pub fn new(path: impl AsRef<str>) -> Self {
        Self {
            active_path: redact_deep_link(path.as_ref()),
        }
    }

    /// Return the normalized active path.
    pub fn active_path(&self) -> &str {
        &self.active_path
    }

    /// Set the active path after a successful navigation request.
    pub fn set_active_path(&mut self, path: impl AsRef<str>) {
        self.active_path = redact_deep_link(path.as_ref());
    }

    /// Check whether an item is active, including nested paths.
    pub fn is_active(&self, item: &NavItem) -> bool {
        is_path_active(&self.active_path, &item.path)
    }
}

/// Normalize a client path without changing its meaning.
pub fn normalize_path(path: &str) -> &str {
    let path = path.trim();
    if path.is_empty() {
        return "/";
    }
    if path.starts_with('/') {
        let normalized = path.trim_end_matches('/');
        if normalized.is_empty() {
            "/"
        } else {
            normalized
        }
    } else {
        path
    }
}

fn is_path_active(active: &str, item: &str) -> bool {
    active == item
        || (item != "/"
            && active.starts_with(item)
            && active.as_bytes().get(item.len()) == Some(&b'/'))
}

#[cfg(test)]
mod tests {
    use super::{
        NavAccess, NavGroup, NavItem, NavigationState, localized_navigation_label,
        localized_navigation_requirement, localized_role_label, normalize_path,
    };

    #[test]
    fn paths_are_normalized_without_losing_nested_routes() {
        assert_eq!(normalize_path(""), "/");
        assert_eq!(normalize_path("/platform/"), "/platform");
        assert_eq!(normalize_path("platform"), "platform");

        let mut state = NavigationState::new("/platform/customers/123/");
        let platform = NavItem::new("Platform", "/platform", "▦");
        let operations = NavItem::new("Operations", "/operations", "⌁");
        assert!(state.is_active(&platform));
        assert!(!state.is_active(&operations));

        state.set_active_path("/operations");
        assert!(state.is_active(&operations));
        assert!(!state.is_active(&platform));
    }

    #[test]
    fn navigation_groups_preserve_explicit_work_area_counts() {
        let group = NavGroup::new(
            "Customer Operations",
            vec![NavItem::new("Customer 360", "/platform/customers", "◎")],
        );
        assert_eq!(group.label, "Customer Operations");
        assert_eq!(group.items.len(), 1);
    }

    #[test]
    fn manifest_metadata_is_bounded_and_navigation_copy_localizes() {
        let item = NavItem::new("Finance", "/workspaces/finance", "module")
            .with_manifest_metadata("finance", "nav.business_unit.finance");
        assert_eq!(item.route_id, "finance");
        assert_eq!(
            localized_navigation_label(&item.label_key, &item.label, crate::Locale::Th),
            "การเงิน"
        );
        let invalid = NavItem::new("Unsafe", "/unsafe", "module")
            .with_manifest_metadata("access_token", "nav.route.SECRET");
        assert!(invalid.route_id.is_empty());
        assert!(invalid.label_key.is_empty());
    }

    #[test]
    fn root_does_not_match_every_nested_path() {
        let state = NavigationState::new("/operations");
        assert!(!state.is_active(&NavItem::new("Home", "/", "⌂")));
    }

    #[test]
    fn navigation_targets_redact_credentials_before_reaching_the_shell() {
        let credential_bearing = NavItem::new(
            "Customer 360",
            "/platform/customers?access_token=secret#details",
            "◎",
        );
        assert_eq!(credential_bearing.path, "/");
        assert!(!credential_bearing.path.contains("secret"));

        let query_only = NavItem::new(
            "Customer 360",
            "/platform/customers?view=health#details",
            "◎",
        );
        assert_eq!(query_only.path, "/platform/customers");
    }

    #[test]
    fn action_capabilities_are_bounded_sorted_and_never_authorization_material() {
        let item = NavItem::new("Quality", "/workspaces/quality", "quality")
            .with_action_capabilities([
                "TransitionQualityInspection",
                "CreateQualityInspection",
                "CreateQualityInspection",
                "access_token",
                "bad capability",
            ]);
        assert_eq!(
            item.action_capabilities,
            ["CreateQualityInspection", "TransitionQualityInspection"]
        );
        assert!(item.allows_action("CreateQualityInspection"));
        assert!(!item.allows_action("DeleteQualityInspection"));
    }

    #[test]
    fn locked_navigation_is_bounded_sorted_and_defaults_remain_authorized() {
        let available = NavItem::new("Finance", "/workspaces/finance", "finance");
        assert!(available.is_authorized());
        assert!(available.required_role_keys().is_empty());

        let locked = available.with_access(NavAccess::locked([
            "finance_viewer",
            "finance_viewer",
            "finance_operator",
            "access_token",
            "bad role",
        ]));
        assert!(!locked.is_authorized());
        assert_eq!(
            locked.required_role_keys(),
            ["finance_operator", "finance_viewer"]
        );
    }

    #[test]
    fn canonical_role_requirements_are_bilingual_and_have_a_safe_fallback() {
        assert_eq!(
            localized_role_label("finance_viewer", crate::Locale::En),
            "Finance Viewer"
        );
        assert_eq!(
            localized_role_label("finance_viewer", crate::Locale::Th),
            "ผู้ดูข้อมูล การเงิน"
        );
        let thai = localized_navigation_requirement(
            &["finance_viewer".to_owned(), "finance_operator".to_owned()],
            crate::Locale::Th,
        );
        assert!(thai.contains("Role/Persona"));
        assert!(thai.contains("การเงิน"));
        assert!(
            localized_navigation_requirement(&[], crate::Locale::En)
                .contains("No standard Role/Persona")
        );
    }

    #[test]
    fn active_paths_reject_external_and_encoded_targets() {
        let state = NavigationState::new("https://evil.example/platform");
        assert_eq!(state.active_path(), "/");

        let mut state = NavigationState::new("/platform");
        state.set_active_path("/platform/%2Fcustomers");
        assert_eq!(state.active_path(), "/");
        state.set_active_path("//evil.example/platform");
        assert_eq!(state.active_path(), "/");
    }

    #[test]
    fn normalize_path_remains_a_non_allocating_display_helper() {
        assert_eq!(normalize_path("/platform/"), "/platform");
    }
}
