//! Marketing-specific signed object access.
//!
//! The caller must already have an authoritative, tenant-scoped access
//! decision. This adapter binds that decision to a validated content asset
//! version and a tenant storage namespace; it never accepts a caller URL,
//! credential, object bytes, or bearer token.

use chrono::{DateTime, Utc};
use erpx_domain::{
    contracts::TenantScope,
    marketing_content::{ContentAssetVersion, MarketingContentError},
};
use thiserror::Error;
use uuid::Uuid;

use crate::{MinioObjectStore, ObjectStorageError, SignedObjectUrl};

pub const MAX_MARKETING_ASSET_SIGNED_URL_SECONDS: u32 = 3_600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingObjectPermission {
    Upload,
    Download,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingObjectAccessContext {
    pub actor_ref: String,
    pub requested_scope: TenantScope,
    pub permission: MarketingObjectPermission,
    pub purpose: String,
    pub correlation_id: Uuid,
}

impl MarketingObjectAccessContext {
    pub fn validate_for(
        &self,
        asset: &ContentAssetVersion,
    ) -> Result<(), MarketingContentObjectError> {
        if self.actor_ref.trim().is_empty()
            || self.actor_ref.len() > 255
            || self.actor_ref.chars().any(char::is_control)
            || self.purpose.trim().is_empty()
            || self.purpose.len() > 512
            || self.purpose.chars().any(char::is_control)
            || self.correlation_id.is_nil()
        {
            return Err(MarketingContentObjectError::InvalidAccessContext);
        }
        if matches!(self.requested_scope, TenantScope::Platform)
            || !self.requested_scope.contains(&asset.scope)
        {
            return Err(MarketingContentObjectError::ScopeMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum MarketingContentObjectError {
    #[error("marketing content asset is invalid")]
    InvalidAsset(#[source] MarketingContentError),
    #[error("marketing object access context is invalid")]
    InvalidAccessContext,
    #[error("marketing object access scope does not contain the asset")]
    ScopeMismatch,
    #[error("marketing object URL lifetime exceeds the bounded limit")]
    InvalidLifetime,
    #[error("marketing object namespace is required")]
    MissingNamespace,
    #[error("marketing object namespace is invalid")]
    InvalidNamespace,
    #[error("object storage operation failed")]
    Storage(#[source] ObjectStorageError),
}

#[derive(Debug, Clone)]
pub struct MarketingContentObjectStore {
    store: MinioObjectStore,
}

impl MarketingContentObjectStore {
    pub fn new(store: MinioObjectStore) -> Self {
        Self { store }
    }

    pub fn presign_upload(
        &self,
        namespace: &str,
        asset: &ContentAssetVersion,
        access: &MarketingObjectAccessContext,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, MarketingContentObjectError> {
        validate_request(
            namespace,
            asset,
            access,
            expires_seconds,
            MarketingObjectPermission::Upload,
        )?;
        self.store
            .presign_upload(namespace, &asset.object_ref.object_key, expires_seconds, at)
            .map_err(MarketingContentObjectError::Storage)
    }

    pub fn presign_download(
        &self,
        namespace: &str,
        asset: &ContentAssetVersion,
        access: &MarketingObjectAccessContext,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, MarketingContentObjectError> {
        validate_request(
            namespace,
            asset,
            access,
            expires_seconds,
            MarketingObjectPermission::Download,
        )?;
        self.store
            .presign_download(namespace, &asset.object_ref.object_key, expires_seconds, at)
            .map_err(MarketingContentObjectError::Storage)
    }
}

fn validate_request(
    namespace: &str,
    asset: &ContentAssetVersion,
    access: &MarketingObjectAccessContext,
    expires_seconds: u32,
    permission: MarketingObjectPermission,
) -> Result<(), MarketingContentObjectError> {
    if namespace.trim().is_empty() {
        return Err(MarketingContentObjectError::MissingNamespace);
    }
    if namespace != namespace.trim()
        || namespace.len() > 255
        || namespace.chars().any(char::is_control)
        || namespace.contains('/')
        || namespace.contains('\\')
        || namespace == "."
        || namespace == ".."
    {
        return Err(MarketingContentObjectError::InvalidNamespace);
    }
    if expires_seconds == 0 || expires_seconds > MAX_MARKETING_ASSET_SIGNED_URL_SECONDS {
        return Err(MarketingContentObjectError::InvalidLifetime);
    }
    asset
        .validate()
        .map_err(MarketingContentObjectError::InvalidAsset)?;
    access.validate_for(asset)?;
    if access.permission != permission {
        return Err(MarketingContentObjectError::InvalidAccessContext);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::marketing_content::StorageObjectRef;

    fn asset(scope: TenantScope) -> ContentAssetVersion {
        ContentAssetVersion::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            scope.clone(),
            1,
            StorageObjectRef::new(scope, "assets/launch.png").expect("object ref"),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            128,
            "image/png",
            "author",
            Utc::now(),
        )
        .expect("asset")
    }

    fn access(
        scope: TenantScope,
        permission: MarketingObjectPermission,
    ) -> MarketingObjectAccessContext {
        MarketingObjectAccessContext {
            actor_ref: "content-editor".into(),
            requested_scope: scope,
            permission,
            purpose: "marketing content test".into(),
            correlation_id: Uuid::new_v4(),
        }
    }

    #[test]
    fn signed_access_binds_scope_permission_and_lifetime() {
        let holding = Uuid::new_v4();
        let scope = TenantScope::Holding {
            holding_id: holding,
        };
        let asset = asset(scope.clone());
        let store = MarketingContentObjectStore::new(
            MinioObjectStore::new(crate::MinioObjectStoreConfig {
                endpoint: "https://minio.example.test".into(),
                access_key: "access".into(),
                secret_key: "secret".into(),
                region: "us-east-1".into(),
            })
            .expect("store"),
        );
        let signed = store
            .presign_download(
                "holding-content",
                &asset,
                &access(scope.clone(), MarketingObjectPermission::Download),
                600,
                Utc::now(),
            )
            .expect("signed download");
        assert_eq!(signed.method, "GET");
        assert!(signed.url.contains("holding-content"));
        assert!(
            store
                .presign_download(
                    "holding-content",
                    &asset,
                    &access(
                        TenantScope::Holding {
                            holding_id: Uuid::new_v4(),
                        },
                        MarketingObjectPermission::Download,
                    ),
                    600,
                    Utc::now(),
                )
                .is_err()
        );
    }

    #[test]
    fn upload_and_download_permissions_are_not_interchangeable() {
        let scope = TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        };
        let asset = asset(scope.clone());
        let access = access(scope, MarketingObjectPermission::Download);
        assert!(matches!(
            validate_request(
                "holding-content",
                &asset,
                &access,
                600,
                MarketingObjectPermission::Upload,
            ),
            Err(MarketingContentObjectError::InvalidAccessContext)
        ));
        assert!(matches!(
            validate_request(
                "holding-content",
                &asset,
                &access,
                MAX_MARKETING_ASSET_SIGNED_URL_SECONDS + 1,
                MarketingObjectPermission::Download,
            ),
            Err(MarketingContentObjectError::InvalidLifetime)
        ));
    }

    #[test]
    fn namespace_path_injection_and_control_values_fail_closed() {
        let scope = TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        };
        let asset = asset(scope.clone());
        let access = access(scope, MarketingObjectPermission::Download);

        for namespace in [
            "holding-content/other",
            "../holding-content",
            "holding-content\\other",
            "holding-content\n",
            " holding-content",
        ] {
            assert!(matches!(
                validate_request(
                    namespace,
                    &asset,
                    &access,
                    600,
                    MarketingObjectPermission::Download,
                ),
                Err(MarketingContentObjectError::InvalidNamespace)
            ));
        }
    }
}
