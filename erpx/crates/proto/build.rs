use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/erpx/common/v1/context.proto");
    println!("cargo:rerun-if-changed=proto/erpx/platform/v1/platform.proto");
    let descriptor_set_path = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?)
        .join("erpx_descriptor_set.bin");
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .build_transport(false)
        .file_descriptor_set_path(descriptor_set_path)
        .compile_protos(
            &[
                "proto/erpx/common/v1/context.proto",
                "proto/erpx/platform/v1/platform.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}
