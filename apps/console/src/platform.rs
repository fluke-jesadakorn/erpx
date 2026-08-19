//! Explicit Native/Blitz platform capability contract.
//!
//! A missing integration is surfaced to the UI. This module contains no
//! WebView bridge and no renderer fallback.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use erpx_delivery_contract::ClientChannel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NativeCapability {
    Supported,
    Degraded { reason: String },
    Unavailable { reason: String },
}

impl NativeCapability {
    pub const fn is_supported(&self) -> bool {
        matches!(self, Self::Supported)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeFeature {
    WindowLifecycle,
    KeyboardPointer,
    Clipboard,
    ExternalUrl,
    FileDialogs,
    SecureStoragePasskeys,
    VoiceOver,
    GrpcTls,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCapabilityMatrix {
    pub channel: ClientChannel,
    pub renderer: String,
    pub features: BTreeMap<NativeFeature, NativeCapability>,
}

impl NativeCapabilityMatrix {
    pub fn for_channel(channel: ClientChannel) -> Self {
        let mut features = BTreeMap::new();
        match channel {
            ClientChannel::MacOs => {
                for feature in [
                    NativeFeature::WindowLifecycle,
                    NativeFeature::KeyboardPointer,
                    NativeFeature::Clipboard,
                    NativeFeature::ExternalUrl,
                    NativeFeature::FileDialogs,
                    NativeFeature::VoiceOver,
                    NativeFeature::GrpcTls,
                ] {
                    features.insert(feature, NativeCapability::Supported);
                }
                features.insert(
                    NativeFeature::SecureStoragePasskeys,
                    NativeCapability::Degraded {
                        reason: "passkeys require a certified AuthenticationServices ceremony"
                            .to_owned(),
                    },
                );
            }
            ClientChannel::Ios | ClientChannel::Android => {
                for feature in [
                    NativeFeature::WindowLifecycle,
                    NativeFeature::KeyboardPointer,
                    NativeFeature::Clipboard,
                    NativeFeature::ExternalUrl,
                    NativeFeature::FileDialogs,
                    NativeFeature::SecureStoragePasskeys,
                    NativeFeature::VoiceOver,
                    NativeFeature::GrpcTls,
                ] {
                    features.insert(
                        feature,
                        NativeCapability::Unavailable {
                            reason: "compile-certified adapter; runtime certification is deferred"
                                .to_owned(),
                        },
                    );
                }
            }
            ClientChannel::Web => {
                for feature in [
                    NativeFeature::WindowLifecycle,
                    NativeFeature::KeyboardPointer,
                    NativeFeature::Clipboard,
                    NativeFeature::ExternalUrl,
                    NativeFeature::FileDialogs,
                    NativeFeature::SecureStoragePasskeys,
                    NativeFeature::VoiceOver,
                    NativeFeature::GrpcTls,
                ] {
                    features.insert(
                        feature,
                        NativeCapability::Unavailable {
                            reason: "not a native client".to_owned(),
                        },
                    );
                }
            }
        }
        Self {
            channel,
            renderer: if matches!(channel, ClientChannel::Web) {
                "web-dom".to_owned()
            } else {
                "dioxus-native-blitz-wgpu".to_owned()
            },
            features,
        }
    }

    pub fn capability(&self, feature: NativeFeature) -> &NativeCapability {
        self.features
            .get(&feature)
            .expect("every platform feature has an explicit status")
    }
}

pub fn native_distribution_enabled() -> bool {
    std::env::var("ERPX_NATIVE_RELEASE_KILL_SWITCH").as_deref() != Ok("disabled")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_is_native_only_and_every_capability_is_explicit() {
        let matrix = NativeCapabilityMatrix::for_channel(ClientChannel::MacOs);
        assert_eq!(matrix.renderer, "dioxus-native-blitz-wgpu");
        assert_eq!(matrix.features.len(), 8);
        assert!(matches!(
            matrix.capability(NativeFeature::SecureStoragePasskeys),
            NativeCapability::Degraded { .. }
        ));
        assert!(!matrix.renderer.to_ascii_lowercase().contains("webview"));
    }

    #[test]
    fn mobile_adapters_compile_with_typed_unavailable_status() {
        for channel in [ClientChannel::Ios, ClientChannel::Android] {
            let matrix = NativeCapabilityMatrix::for_channel(channel);
            assert!(
                matrix
                    .features
                    .values()
                    .all(|capability| matches!(capability, NativeCapability::Unavailable { .. }))
            );
        }
    }
}
