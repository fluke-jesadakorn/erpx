//! Pure validation of explicitly registered module and external API/event contracts.
use semver::{Version, VersionReq};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionedContract {
    pub name: String,
    pub version: String,
    pub source: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequiredContract {
    pub name: String,
    pub range: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleDescriptor {
    pub id: String,
    pub version: String,
    pub group: String,
    pub provides: Vec<VersionedContract>,
    pub requires: Vec<RequiredContract>,
    pub events: Vec<VersionedContract>,
    pub requires_events: Vec<RequiredContract>,
}
/// External descriptors declare contracts only; DB handles and local factories are not accepted.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalContract {
    pub name: String,
    pub version: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalDescriptor {
    pub id: String,
    pub api: Vec<ExternalContract>,
    pub events: Vec<ExternalContract>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleCatalog {
    pub version: u32,
    pub modules: Vec<ModuleDescriptor>,
    pub external: Vec<ExternalDescriptor>,
}
fn version(value: &str) -> Result<Version, &'static str> {
    Version::parse(value).map_err(|_| "MODULE_VERSION_INVALID")
}
fn insert_contract(
    map: &mut BTreeMap<String, Version>,
    name: &str,
    ver: &str,
) -> Result<(), &'static str> {
    if name.is_empty() || map.insert(name.to_owned(), version(ver)?).is_some() {
        return Err("MODULE_CONTRACT_DUPLICATE");
    }
    Ok(())
}
fn requirements(
    map: &BTreeMap<String, Version>,
    items: &[RequiredContract],
) -> Result<(), &'static str> {
    for req in items {
        let range = VersionReq::parse(&req.range).map_err(|_| "MODULE_RANGE_INVALID")?;
        if range.comparators.is_empty() {
            return Err("MODULE_RANGE_INVALID");
        }
        let found = map.get(&req.name).ok_or("MODULE_CONTRACT_MISSING")?;
        if !range.matches(found) {
            return Err("MODULE_CONTRACT_INCOMPATIBLE");
        }
    }
    Ok(())
}
pub fn validate(catalog: &ModuleCatalog, enabled: &[&str]) -> Result<(), &'static str> {
    if catalog.version != 1 {
        return Err("MODULE_CATALOG_INVALID");
    }
    let mut ids = BTreeSet::new();
    for m in &catalog.modules {
        if m.id.is_empty()
            || !ids.insert(m.id.as_str())
            || !matches!(m.group.as_str(), "core" | "existing-business" | "example")
        {
            return Err("MODULE_DESCRIPTOR_INVALID");
        }
        version(&m.version)?;
    }
    let selected: BTreeSet<_> = enabled.iter().copied().collect();
    if selected.len() != enabled.len() || selected.iter().any(|id| !ids.contains(id)) {
        return Err("MODULE_SELECTION_INVALID");
    }
    let mut api = BTreeMap::new();
    let mut events = BTreeMap::new();
    for m in catalog
        .modules
        .iter()
        .filter(|m| selected.contains(m.id.as_str()))
    {
        let module_version = version(&m.version)?;
        for c in &m.provides {
            if c.source.is_empty() || c.name.starts_with("external.") {
                return Err("MODULE_DESCRIPTOR_INVALID");
            }
            if version(&c.version)? != module_version {
                return Err("MODULE_CONTRACT_INCOMPATIBLE");
            }
            insert_contract(&mut api, &c.name, &c.version)?
        }
        for c in &m.events {
            if c.source.is_empty() || c.name.starts_with("external.") {
                return Err("MODULE_DESCRIPTOR_INVALID");
            }
            insert_contract(&mut events, &c.name, &c.version)?
        }
    }
    let mut external_ids = BTreeSet::new();
    for external in &catalog.external {
        if external.id.is_empty()
            || !external_ids.insert(external.id.as_str())
            || ids.contains(external.id.as_str())
        {
            return Err("MODULE_DESCRIPTOR_INVALID");
        }
        for c in &external.api {
            if !c.name.starts_with("external.") {
                return Err("MODULE_DESCRIPTOR_INVALID");
            }
            insert_contract(&mut api, &c.name, &c.version)?
        }
        for c in &external.events {
            if !c.name.starts_with("external.") {
                return Err("MODULE_DESCRIPTOR_INVALID");
            }
            insert_contract(&mut events, &c.name, &c.version)?
        }
    }
    for m in catalog
        .modules
        .iter()
        .filter(|m| selected.contains(m.id.as_str()))
    {
        requirements(&api, &m.requires)?;
        requirements(&events, &m.requires_events)?
    }
    Ok(())
}
