use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

const PROTO_SOURCE_FILES: &[&str] = &[
    "proto/erpx/common/v1/context.proto",
    "proto/erpx/platform/v1/asset_maintenance.proto",
    "proto/erpx/platform/v1/assets.proto",
    "proto/erpx/platform/v1/brand.proto",
    "proto/erpx/platform/v1/commercial.proto",
    "proto/erpx/platform/v1/crm.proto",
    "proto/erpx/platform/v1/customer.proto",
    "proto/erpx/platform/v1/customer_360.proto",
    "proto/erpx/platform/v1/data_plane.proto",
    "proto/erpx/platform/v1/finance.proto",
    "proto/erpx/platform/v1/identity_access.proto",
    "proto/erpx/platform/v1/inventory.proto",
    "proto/erpx/platform/v1/legal.proto",
    "proto/erpx/platform/v1/marketing.proto",
    "proto/erpx/platform/v1/maintenance.proto",
    "proto/erpx/platform/v1/operations.proto",
    "proto/erpx/platform/v1/organization.proto",
    "proto/erpx/platform/v1/people_payroll.proto",
    "proto/erpx/platform/v1/platform_administration.proto",
    "proto/erpx/platform/v1/pos.proto",
    "proto/erpx/platform/v1/procurement.proto",
    "proto/erpx/platform/v1/quality.proto",
    "proto/erpx/platform/v1/sales.proto",
    "proto/erpx/platform/v1/supply_chain.proto",
    "proto/erpx/platform/v1/support.proto",
    "proto/erpx/platform/v1/platform.proto",
];

const PROTO_ENTRYPOINTS: &[&str] = &[
    "proto/erpx/common/v1/context.proto",
    "proto/erpx/platform/v1/platform.proto",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for proto_file in PROTO_SOURCE_FILES {
        println!("cargo:rerun-if-changed={proto_file}");
    }
    println!("cargo:rerun-if-changed=descriptor/erpx_descriptor_set.sha256");
    let descriptor_set_path = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?)
        .join("erpx_descriptor_set.bin");
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .build_transport(false)
        .boxed(".erpx.platform.v1.DataPlaneAgentEnvelope.payload.observation")
        .file_descriptor_set_path(&descriptor_set_path)
        .compile_protos(PROTO_ENTRYPOINTS, &["proto"])?;
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
