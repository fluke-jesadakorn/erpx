//! secure transport is private to the Directory adapter.
//! Operator profile data intentionally has no Debug/Serialize implementation.
use super::directory_ldap_wire::{LdapFailure, SequentialLdap};
use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, ServerName, pem::PemObject},
};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio::{
    net::TcpStream,
    time::{Instant, timeout_at},
};
use tokio_rustls::{TlsConnector, client::TlsStream};

pub(super) struct LdapsTransportPolicy {
    host: String,
    port: u16,
    tls_name: ServerName<'static>,
    config: Arc<ClientConfig>,
}
impl LdapsTransportPolicy {
    /// Called only after actual selected profile.connectionIds includes this
    /// connection. Explicit caPem replaces public roots, matching B0 Node TLS.
    /// SNI may differ from the approved socket hostname, but is always verified.
    pub(super) fn checked(
        endpoint: &str,
        allowed_hosts: &BTreeSet<String>,
        ca_pem: Option<&str>,
        tls_server_name: Option<&str>,
    ) -> Result<Self, LdapFailure> {
        if endpoint.len() > 8192 {
            return Err(LdapFailure::Limit);
        }
        let url = url::Url::parse(endpoint).map_err(|_| LdapFailure::Malformed)?;
        let approved_host = url.host_str().ok_or(LdapFailure::Malformed)?;
        let host = match url.host().ok_or(LdapFailure::Malformed)? {
            url::Host::Domain(host) => host.to_owned(),
            url::Host::Ipv4(host) => host.to_string(),
            url::Host::Ipv6(host) => host.to_string(),
        };
        if url.scheme() != "ldaps"
            || !url.username().is_empty()
            || url.password().is_some()
            || !allowed_hosts.contains(&approved_host.to_lowercase())
        {
            return Err(LdapFailure::Malformed);
        }
        let tls_name = ServerName::try_from(tls_server_name.unwrap_or(&host).to_owned())
            .map_err(|_| LdapFailure::Malformed)?;
        let mut roots = if ca_pem.is_some() {
            RootCertStore::empty()
        } else {
            RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned())
        };
        if let Some(pem) = ca_pem {
            if pem.len() > 65_536 {
                return Err(LdapFailure::Limit);
            }
            let mut count = 0usize;
            for certificate in CertificateDer::pem_slice_iter(pem.as_bytes()) {
                count += 1;
                if count > 16 {
                    return Err(LdapFailure::Limit);
                }
                roots
                    .add(certificate.map_err(|_| LdapFailure::Malformed)?)
                    .map_err(|_| LdapFailure::Malformed)?;
            }
            if count == 0 {
                return Err(LdapFailure::Malformed);
            }
        }
        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|_| LdapFailure::Malformed)?
                .with_root_certificates(roots)
                .with_no_client_auth();
        Ok(Self {
            host,
            port: url.port().unwrap_or(636),
            tls_name,
            config: Arc::new(config),
        })
    }
    pub(super) async fn connect(
        &self,
    ) -> Result<SequentialLdap<TlsStream<TcpStream>>, LdapFailure> {
        let started = Instant::now();
        let deadline = started + Duration::from_secs(5);
        // TCP resolution/connect and certificate handshake share one deadline.
        // No environment proxy, redirects, fallback plaintext, custom verifier,
        // global crypto provider installation or referral endpoints exist here.
        let stream = timeout_at(
            deadline,
            TcpStream::connect((self.host.as_str(), self.port)),
        )
        .await
        .map_err(|_| LdapFailure::TimedOut)?
        .map_err(|_| LdapFailure::Transport)?;
        let tls = timeout_at(
            deadline,
            TlsConnector::from(self.config.clone()).connect(self.tls_name.clone(), stream),
        )
        .await
        .map_err(|_| LdapFailure::TimedOut)?
        .map_err(|_| LdapFailure::Transport)?;
        Ok(SequentialLdap::new(tls, started))
    }
}
