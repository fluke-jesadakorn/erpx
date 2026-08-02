use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/erpx/common/v1/context.proto");
    println!("cargo:rerun-if-changed=proto/erpx/platform/v1/platform.proto");
    println!("cargo:rerun-if-changed=descriptor/erpx_descriptor_set.sha256");
    let descriptor_set_path = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?)
        .join("erpx_descriptor_set.bin");
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .build_transport(false)
        .boxed(".erpx.platform.v1.DataPlaneAgentEnvelope.payload.observation")
        .file_descriptor_set_path(&descriptor_set_path)
        .compile_protos(
            &[
                "proto/erpx/common/v1/context.proto",
                "proto/erpx/platform/v1/platform.proto",
            ],
            &["proto"],
        )?;
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
