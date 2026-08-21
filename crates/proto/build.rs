use prost::Message;
use prost_types::{
    DescriptorProto, EnumDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    ServiceDescriptorProto,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env, fs, path::PathBuf};

const ACTIVE_PROTO_SOURCE_FILES: &[&str] = &[
    "proto/erpx/common/context.proto",
    "proto/erpx/interface/reference.proto",
    "proto/erpx/identity_access/service.proto",
    "proto/erpx/platform_administration/service.proto",
    "proto/erpx/platform_administration/data_plane.proto",
    "proto/erpx/organization/service.proto",
    "proto/erpx/commercial/service.proto",
    "proto/erpx/brand/service.proto",
    "proto/erpx/customer_authority/service.proto",
    "proto/erpx/customer_360/service.proto",
    "proto/erpx/support/service.proto",
    "proto/erpx/finance/service.proto",
    "proto/erpx/sales/service.proto",
    "proto/erpx/pos/service.proto",
    "proto/erpx/procurement/service.proto",
    "proto/erpx/inventory/service.proto",
    "proto/erpx/assets/service.proto",
    "proto/erpx/maintenance/service.proto",
    "proto/erpx/quality/service.proto",
    "proto/erpx/operations/service.proto",
    "proto/erpx/people_payroll/service.proto",
    "proto/erpx/legal/service.proto",
    "proto/erpx/marketing/service.proto",
    "proto/erpx/crm/service.proto",
    "proto/erpx/manufacturing/service.proto",
    "proto/erpx/planning/service.proto",
    "proto/erpx/projects/service.proto",
    "proto/erpx/ecommerce/service.proto",
    "proto/erpx/talent/service.proto",
    "proto/erpx/learning/service.proto",
    "proto/erpx/benefits/service.proto",
    "proto/erpx/field_service/service.proto",
    "proto/erpx/collaboration/service.proto",
    "proto/erpx/agent/service.proto",
    "proto/erpx/reference/service.proto",
    "proto/erpx/catalog/active.proto",
];

const ACTIVE_PROTO_ENTRYPOINTS: &[&str] = &["proto/erpx/catalog/active.proto"];
const INFRASTRUCTURE_PROTO_ENTRYPOINTS: &[&str] =
    &["proto/erpx/platform_administration/data_plane_agent/service.proto"];

// Scoped only to tonic-generated server modules. Hand-written ERPX sources
// remain warnings-denied and receive no workspace-wide Clippy exception.
const GENERATED_SERVER_CLIPPY_ALLOW: &str =
    "#[allow(clippy::match_single_binding, clippy::mixed_attributes_style)]";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for proto_file in ACTIVE_PROTO_SOURCE_FILES {
        println!("cargo:rerun-if-changed={proto_file}");
    }
    for proto_file in INFRASTRUCTURE_PROTO_ENTRYPOINTS {
        println!("cargo:rerun-if-changed={proto_file}");
    }
    println!("cargo:rerun-if-changed=descriptor/erpx_descriptor_set.sha256");
    println!("cargo:rerun-if-changed=descriptor/erpx_prior_descriptor_set.bin");
    println!("cargo:rerun-if-changed=descriptor/erpx_prior_descriptor_set.sha256");
    let descriptor_set_path = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?)
        .join("erpx_descriptor_set.bin");
    // Only the unversioned interface families enter the approved descriptor
    // and generated Rust surface. Retired contracts are never generated as aliases.
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .build_transport(false)
        .server_mod_attribute(".", GENERATED_SERVER_CLIPPY_ALLOW)
        // This is attached only to Prost's generated oneof enum. The wire
        // contract intentionally keeps the observation payload inline, while
        // handwritten ERPX code remains warnings-denied.
        .type_attribute(
            "erpx.platform_administration.DataPlaneAgentEnvelope.payload",
            "#[allow(clippy::large_enum_variant)]",
        )
        .file_descriptor_set_path(&descriptor_set_path)
        .compile_protos(ACTIVE_PROTO_ENTRYPOINTS, &["proto"])?;

    // The outbound customer-hosted agent protocol is an infrastructure host
    // contract, not an additional business service ID. Generate it separately
    // so it cannot enter the approved 67-service reflection descriptor.
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .build_transport(false)
        .server_mod_attribute(".", GENERATED_SERVER_CLIPPY_ALLOW)
        .extern_path(
            ".erpx.platform_administration",
            "crate::erpx::platform_administration",
        )
        .compile_protos(INFRASTRUCTURE_PROTO_ENTRYPOINTS, &["proto"])?;
    let generated = fs::read(&descriptor_set_path)?;
    let prior = fs::read("descriptor/erpx_prior_descriptor_set.bin")?;
    let prior_hash = format!("{:x}", Sha256::digest(&prior));
    let approved_prior_hash = fs::read_to_string("descriptor/erpx_prior_descriptor_set.sha256")?;
    if prior_hash != approved_prior_hash.trim() {
        return Err(format!(
            "prior protobuf descriptor differs from its approved baseline: found {prior_hash}"
        )
        .into());
    }
    verify_wire_compatibility(&prior, &generated)?;
    let generated_hash = format!("{:x}", Sha256::digest(generated));
    let approved_hash = fs::read_to_string("descriptor/erpx_descriptor_set.sha256")?;
    if generated_hash != approved_hash.trim() {
        return Err(format!(
            "protobuf descriptor differs from the approved baseline: generated {generated_hash}"
        )
        .into());
    }
    Ok(())
}

fn verify_wire_compatibility(
    prior: &[u8],
    generated: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let prior = FileDescriptorSet::decode(prior)?;
    let generated = FileDescriptorSet::decode(generated)?;
    let current_files = generated
        .file
        .iter()
        .filter_map(|file| file.name.as_deref().map(|name| (name, file)))
        .collect::<BTreeMap<_, _>>();

    for old_file in &prior.file {
        let file_name = required(old_file.name.as_deref(), "prior file name")?;
        let new_file = current_files.get(file_name).ok_or_else(|| {
            format!("protobuf compatibility failure: removed prior file {file_name}")
        })?;
        compare_file(file_name, old_file, new_file)?;
    }
    Ok(())
}

fn compare_file(
    file_name: &str,
    old: &FileDescriptorProto,
    new: &FileDescriptorProto,
) -> Result<(), Box<dyn std::error::Error>> {
    if old.package != new.package || old.syntax != new.syntax {
        return Err(format!(
            "protobuf compatibility failure: package or syntax changed for {file_name}"
        )
        .into());
    }
    compare_named(
        file_name,
        "service",
        &old.service,
        &new.service,
        compare_service,
    )?;
    compare_named(
        file_name,
        "message",
        &old.message_type,
        &new.message_type,
        compare_message,
    )?;
    compare_named(
        file_name,
        "enum",
        &old.enum_type,
        &new.enum_type,
        compare_enum,
    )?;
    Ok(())
}

trait NamedDescriptor {
    fn descriptor_name(&self) -> Option<&str>;
}

impl NamedDescriptor for ServiceDescriptorProto {
    fn descriptor_name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

impl NamedDescriptor for DescriptorProto {
    fn descriptor_name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

impl NamedDescriptor for EnumDescriptorProto {
    fn descriptor_name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

fn compare_named<T, F>(
    parent: &str,
    kind: &str,
    old: &[T],
    new: &[T],
    mut compare: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: NamedDescriptor,
    F: FnMut(&str, &T, &T) -> Result<(), Box<dyn std::error::Error>>,
{
    let current = new
        .iter()
        .filter_map(|descriptor| descriptor.descriptor_name().map(|name| (name, descriptor)))
        .collect::<BTreeMap<_, _>>();
    for old_descriptor in old {
        let name = required(old_descriptor.descriptor_name(), "prior descriptor name")?;
        let path = format!("{parent}.{name}");
        let new_descriptor = current.get(name).ok_or_else(|| {
            format!("protobuf compatibility failure: removed prior {kind} {path}")
        })?;
        compare(&path, old_descriptor, new_descriptor)?;
    }
    Ok(())
}

fn compare_service(
    path: &str,
    old: &ServiceDescriptorProto,
    new: &ServiceDescriptorProto,
) -> Result<(), Box<dyn std::error::Error>> {
    let current = new
        .method
        .iter()
        .filter_map(|method| method.name.as_deref().map(|name| (name, method)))
        .collect::<BTreeMap<_, _>>();
    for old_method in &old.method {
        let name = required(old_method.name.as_deref(), "prior method name")?;
        let method_path = format!("{path}.{name}");
        let new_method = current.get(name).ok_or_else(|| {
            format!("protobuf compatibility failure: removed prior method {method_path}")
        })?;
        if old_method.input_type != new_method.input_type
            || old_method.output_type != new_method.output_type
            || old_method.client_streaming != new_method.client_streaming
            || old_method.server_streaming != new_method.server_streaming
        {
            return Err(format!(
                "protobuf compatibility failure: method signature changed for {method_path}"
            )
            .into());
        }
    }
    Ok(())
}

fn compare_message(
    path: &str,
    old: &DescriptorProto,
    new: &DescriptorProto,
) -> Result<(), Box<dyn std::error::Error>> {
    let current_fields = new
        .field
        .iter()
        .filter_map(|field| field.number.map(|number| (number, field)))
        .collect::<BTreeMap<_, _>>();
    for old_field in &old.field {
        let number = old_field.number.ok_or_else(|| {
            format!("protobuf compatibility failure: field without number in {path}")
        })?;
        let field_path = format!(
            "{path}.{}#{number}",
            old_field.name.as_deref().unwrap_or("<unnamed>")
        );
        let new_field = current_fields.get(&number).ok_or_else(|| {
            format!("protobuf compatibility failure: removed prior field {field_path}")
        })?;
        if old_field.name != new_field.name
            || old_field.label != new_field.label
            || old_field.r#type != new_field.r#type
            || old_field.type_name != new_field.type_name
            || old_field.extendee != new_field.extendee
            || old_field.default_value != new_field.default_value
            || old_field.oneof_index != new_field.oneof_index
            || old_field.proto3_optional != new_field.proto3_optional
        {
            return Err(format!(
                "protobuf compatibility failure: field number was reused or changed for {field_path}"
            )
            .into());
        }
    }
    if old.oneof_decl.len() > new.oneof_decl.len()
        || old
            .oneof_decl
            .iter()
            .zip(&new.oneof_decl)
            .any(|(old, new)| old.name != new.name)
    {
        return Err(format!(
            "protobuf compatibility failure: prior oneof declaration changed for {path}"
        )
        .into());
    }
    compare_named(
        path,
        "nested message",
        &old.nested_type,
        &new.nested_type,
        compare_message,
    )?;
    compare_named(
        path,
        "nested enum",
        &old.enum_type,
        &new.enum_type,
        compare_enum,
    )?;
    Ok(())
}

fn compare_enum(
    path: &str,
    old: &EnumDescriptorProto,
    new: &EnumDescriptorProto,
) -> Result<(), Box<dyn std::error::Error>> {
    let current = new
        .value
        .iter()
        .filter_map(|value| value.number.map(|number| (number, value)))
        .collect::<BTreeMap<_, _>>();
    for old_value in &old.value {
        let number = old_value.number.ok_or_else(|| {
            format!("protobuf compatibility failure: enum value without number in {path}")
        })?;
        let new_value = current.get(&number).ok_or_else(|| {
            format!("protobuf compatibility failure: removed prior enum number {path}#{number}")
        })?;
        if old_value.name != new_value.name {
            return Err(format!(
                "protobuf compatibility failure: enum number was reused for {path}#{number}"
            )
            .into());
        }
    }
    Ok(())
}

fn required<'a>(
    value: Option<&'a str>,
    label: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    value.ok_or_else(|| format!("protobuf compatibility failure: missing {label}").into())
}
