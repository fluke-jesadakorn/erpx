use crate::domain::company_plugins::{
    PLUGIN_MANIFEST_PATH, PluginPin, VerifiedPluginVersion, read_plugin_registry,
    verify_plugin_version,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use reqwest::{Client, Url, header::HeaderValue};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};

const REGISTRY: &str =
    "https://api.github.com/repos/fluke-jesadakorn/erpx/contents/registry.json?ref=plugin-registry";

#[derive(Clone)]
pub struct GithubPluginCatalogue {
    client: Client,
    authorization: Option<HeaderValue>,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLUGIN_CATALOGUE_UNAVAILABLE",
        "The approved plugin catalogue could not be read.",
    )
}

impl GithubPluginCatalogue {
    pub fn new(token: Option<&str>) -> Result<Self, BusinessError> {
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| unavailable())?
        .with_root_certificates(rustls::RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
        ))
        .with_no_client_auth();
        let client = Client::builder()
            .tls_backend_preconfigured(tls)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .referer(false)
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(2))
            .pool_max_idle_per_host(2)
            .build()
            .map_err(|_| unavailable())?;
        let authorization = token
            .map(|token| {
                HeaderValue::from_str(&format!("Bearer {token}")).map(|mut header| {
                    header.set_sensitive(true);
                    header
                })
            })
            .transpose()
            .map_err(|_| unavailable())?;
        Ok(Self {
            client,
            authorization,
        })
    }

    async fn read(&self, url: Url, raw: bool, limit: usize) -> Result<Vec<u8>, BusinessError> {
        let mut request = self
            .client
            .get(url)
            .header("User-Agent", "ERPX-Plugin-Catalogue")
            .header(
                "Accept",
                if raw {
                    "application/vnd.github.raw+json"
                } else {
                    "application/vnd.github+json"
                },
            );
        if let Some(value) = &self.authorization {
            request = request.header("Authorization", value);
        }
        let mut response = request.send().await.map_err(|_| unavailable())?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length > limit as u64)
        {
            return Err(unavailable());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if chunk.len() > limit - bytes.len() {
                return Err(unavailable());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    pub async fn versions(&self) -> Result<Vec<PluginPin>, BusinessError> {
        let url = Url::parse(REGISTRY).map_err(|_| unavailable())?;
        read_plugin_registry(&self.read(url, true, 256 * 1024).await?)
    }

    /// Refresh the approval registry at install time. Never resolve a commit
    /// or trust a download URL from the caller or GitHub's search response.
    pub async fn approved_version(
        &self,
        id: &str,
        version: &str,
    ) -> Result<VerifiedPluginVersion, BusinessError> {
        let pin = self
            .versions()
            .await?
            .into_iter()
            .find(|pin| pin.id == id && pin.version == version)
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "PLUGIN_VERSION_NOT_APPROVED",
                    "This plugin version is not approved.",
                )
            })?;
        let repo = Url::parse(&format!("https://api.github.com/repos/{}", pin.repository))
            .map_err(|_| unavailable())?;
        #[derive(Deserialize)]
        struct Repository {
            id: u64,
            full_name: String,
            archived: bool,
            private: bool,
        }
        let repository: Repository =
            serde_json::from_slice(&self.read(repo, false, 40 * 1024).await?)
                .map_err(|_| unavailable())?;
        if repository.id != pin.repository_id
            || repository.archived
            || repository.private
            || !repository.full_name.eq_ignore_ascii_case(&pin.repository)
        {
            return Err(unavailable());
        }
        let manifest = Url::parse(&format!(
            "https://api.github.com/repos/{}/contents/{}?ref={}",
            pin.repository, PLUGIN_MANIFEST_PATH, pin.commit
        ))
        .map_err(|_| unavailable())?;
        let bytes = self.read(manifest, true, 16 * 1024).await?;
        verify_plugin_version(pin, repository.id, &bytes)
    }
}
