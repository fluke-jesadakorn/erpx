use openssl::{
    ocsp::OcspResponse,
    x509::{X509, X509Crl},
};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

/// One approved operator-owned item. Content is public trust material, never a private key.
#[derive(Clone, Debug)]
pub struct TrustMaterial {
    pub path: String,
    pub content: Vec<u8>,
}

/// This store never imports the OS/browser roots, document roots, or AIA URLs.
pub struct OperatorTrustStore {
    pub(super) roots: Vec<X509>,
    pub(super) intermediates: Vec<X509>,
    pub(super) crls: Vec<(X509Crl, Vec<u8>)>,
    pub(super) ocsps: Vec<OcspResponse>,
    pub(super) digest: String,
}

impl OperatorTrustStore {
    pub fn from_materials(mut materials: Vec<TrustMaterial>) -> Result<Self, String> {
        if materials.len() > 100 {
            return Err("TRUST_MATERIAL_LIMIT".to_owned());
        }
        materials.sort_by(|left, right| left.path.cmp(&right.path));
        let mut store = Self {
            roots: Vec::new(),
            intermediates: Vec::new(),
            crls: Vec::new(),
            ocsps: Vec::new(),
            digest: String::new(),
        };
        let mut manifest = Vec::new();
        let mut previous = None;
        for material in materials {
            if previous.as_ref() == Some(&material.path) {
                return Err("DUPLICATE_TRUST_MATERIAL".to_owned());
            }
            if material.content.len() > 4 * 1024 * 1024 {
                return Err("TRUST_MATERIAL_LIMIT".to_owned());
            }
            let (directory, name) = material
                .path
                .split_once('/')
                .ok_or("INVALID_TRUST_MATERIAL_PATH")?;
            if name.is_empty() || name.contains(['/', '\\']) || matches!(name, "." | "..") {
                return Err("INVALID_TRUST_MATERIAL_PATH".to_owned());
            }
            match directory {
                "roots" | "intermediates" if name.ends_with(".pem") => {
                    let certificates = X509::stack_from_pem(&material.content)
                        .map_err(|_| "INVALID_TRUST_CERTIFICATE")?;
                    if certificates.is_empty() {
                        return Err("INVALID_TRUST_CERTIFICATE".to_owned());
                    }
                    if directory == "roots" {
                        store.roots.extend(certificates);
                    } else {
                        store.intermediates.extend(certificates);
                    }
                }
                "revocation" if name.ends_with(".crl") => {
                    let crl =
                        X509Crl::from_der(&material.content).map_err(|_| "INVALID_TRUST_CRL")?;
                    store.crls.push((crl, material.content.clone()));
                }
                "revocation" if name.ends_with(".ocsp") => {
                    store.ocsps.push(
                        OcspResponse::from_der(&material.content)
                            .map_err(|_| "INVALID_TRUST_OCSP")?,
                    );
                }
                _ => return Err("INVALID_TRUST_MATERIAL_PATH".to_owned()),
            }
            let checksum = hex::encode(Sha256::digest(&material.content));
            // Match the historical manifest's JSON tuple framing (including spaces).
            manifest.push(format!(
                "[{}, {}]",
                serde_json::to_string(&material.path).map_err(|_| "TRUST_MANIFEST")?,
                serde_json::to_string(&checksum).map_err(|_| "TRUST_MANIFEST")?
            ));
            previous = Some(material.path);
        }
        if store.roots.len() + store.intermediates.len() > 100 {
            return Err("TRUST_CERTIFICATE_LIMIT".to_owned());
        }
        store.digest = hex::encode(Sha256::digest(
            format!("[{}]", manifest.join(", ")).as_bytes(),
        ));
        Ok(store)
    }

    /// Read a prepared private trust directory; symbolic links and special files are refused.
    pub fn from_directory(directory: &Path) -> Result<Self, String> {
        let mut materials = Vec::new();
        for kind in ["roots", "intermediates", "revocation"] {
            let entries = match fs::read_dir(directory.join(kind)) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err("TRUST_DIRECTORY_UNAVAILABLE".to_owned()),
            };
            for entry in entries {
                let entry = entry.map_err(|_| "TRUST_DIRECTORY_UNAVAILABLE")?;
                if !entry
                    .file_type()
                    .map_err(|_| "TRUST_DIRECTORY_UNAVAILABLE")?
                    .is_file()
                {
                    return Err("TRUST_DIRECTORY_SPECIAL_FILE".to_owned());
                }
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "INVALID_TRUST_MATERIAL_PATH")?;
                let content = fs::read(entry.path()).map_err(|_| "TRUST_DIRECTORY_UNAVAILABLE")?;
                materials.push(TrustMaterial {
                    path: format!("{kind}/{name}"),
                    content,
                });
                if materials.len() > 100 {
                    return Err("TRUST_MATERIAL_LIMIT".to_owned());
                }
            }
        }
        Self::from_materials(materials)
    }
}
