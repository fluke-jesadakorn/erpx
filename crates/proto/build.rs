use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

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
