#![doc = "Unversioned ERPX Tonic contracts."]

/// The descriptor set generated from the checked-in protobuf sources. The
/// build script compares it with the approved development baseline.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/erpx_descriptor_set.bin"));

pub const APPROVED_DESCRIPTOR_SHA256: &str =
    include_str!("../descriptor/erpx_descriptor_set.sha256");

pub const ACTIVE_SERVICE_COUNT: usize = 65;
pub mod erpx {
    pub mod common {
        tonic::include_proto!("erpx.common");
    }

    pub mod interface {
        tonic::include_proto!("erpx.interface");
    }

    pub mod identity_access {
        tonic::include_proto!("erpx.identity_access");
    }
    pub mod platform_administration {
        tonic::include_proto!("erpx.platform_administration");
        pub mod data_plane_agent {
            tonic::include_proto!("erpx.platform_administration.data_plane_agent");
        }
    }
    pub mod organization {
        tonic::include_proto!("erpx.organization");
    }
    pub mod commercial {
        tonic::include_proto!("erpx.commercial");
    }
    pub mod brand {
        tonic::include_proto!("erpx.brand");
    }
    pub mod customer_authority {
        tonic::include_proto!("erpx.customer_authority");
    }
    pub mod customer_360 {
        tonic::include_proto!("erpx.customer_360");
    }
    pub mod support {
        tonic::include_proto!("erpx.support");
    }
    pub mod finance {
        tonic::include_proto!("erpx.finance");
    }
    pub mod sales {
        tonic::include_proto!("erpx.sales");
    }
    pub mod pos {
        tonic::include_proto!("erpx.pos");
    }
    pub mod procurement {
        tonic::include_proto!("erpx.procurement");
    }
    pub mod inventory {
        tonic::include_proto!("erpx.inventory");
    }
    pub mod assets {
        tonic::include_proto!("erpx.assets");
    }
    pub mod maintenance {
        tonic::include_proto!("erpx.maintenance");
    }
    pub mod quality {
        tonic::include_proto!("erpx.quality");
    }
    pub mod operations {
        tonic::include_proto!("erpx.operations");
    }
    pub mod people_payroll {
        tonic::include_proto!("erpx.people_payroll");
    }
    pub mod legal {
        tonic::include_proto!("erpx.legal");
    }
    pub mod marketing {
        tonic::include_proto!("erpx.marketing");
    }
    pub mod crm {
        tonic::include_proto!("erpx.crm");
    }
    pub mod manufacturing {
        tonic::include_proto!("erpx.manufacturing");
    }
    pub mod planning {
        tonic::include_proto!("erpx.planning");
    }
    pub mod projects {
        tonic::include_proto!("erpx.projects");
    }
    pub mod ecommerce {
        tonic::include_proto!("erpx.ecommerce");
    }
    pub mod talent {
        tonic::include_proto!("erpx.talent");
    }
    pub mod learning {
        tonic::include_proto!("erpx.learning");
    }
    pub mod benefits {
        tonic::include_proto!("erpx.benefits");
    }
    pub mod field_service {
        tonic::include_proto!("erpx.field_service");
    }
    pub mod collaboration {
        tonic::include_proto!("erpx.collaboration");
    }
    pub mod agent {
        tonic::include_proto!("erpx.agent");
    }
    pub mod reference {
        tonic::include_proto!("erpx.reference");
    }
}

pub mod common {
    pub use crate::erpx::common::*;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use erpx_integration_contracts::catalog::ACTIVE_SERVICES;
    use prost::Message as _;
    use prost_types::FileDescriptorSet;

    use super::*;

    #[test]
    fn approved_descriptor_contains_exactly_the_replacement_service_catalog() {
        let descriptor = FileDescriptorSet::decode(FILE_DESCRIPTOR_SET)
            .expect("approved descriptor must decode");
        let actual = descriptor
            .file
            .iter()
            .flat_map(|file| {
                let package = file.package.as_deref().unwrap_or_default();
                file.service.iter().filter_map(move |service| {
                    service
                        .name
                        .as_deref()
                        .map(|name| (package.to_owned(), name.to_owned()))
                })
            })
            .collect::<BTreeSet<_>>();
        let expected = ACTIVE_SERVICES
            .iter()
            .map(|service| (service.package.to_owned(), service.name.to_owned()))
            .collect::<BTreeSet<_>>();

        assert_eq!(actual.len(), ACTIVE_SERVICE_COUNT);
        assert_eq!(actual, expected);
        assert!(
            actual
                .iter()
                .all(|(package, _)| package != concat!("erpx.", "platform"))
        );
    }

    #[test]
    fn every_active_service_exposes_a_real_rpc_method() {
        let descriptor = FileDescriptorSet::decode(FILE_DESCRIPTOR_SET)
            .expect("approved descriptor must decode");
        let methods_by_service = descriptor
            .file
            .iter()
            .flat_map(|file| {
                let package = file.package.as_deref().unwrap_or_default();
                file.service.iter().filter_map(move |service| {
                    service
                        .name
                        .as_deref()
                        .map(|name| ((package.to_owned(), name.to_owned()), service.method.len()))
                })
            })
            .collect::<std::collections::BTreeMap<_, _>>();

        for service in ACTIVE_SERVICES {
            let key = (service.package.to_owned(), service.name.to_owned());
            let method_count = methods_by_service
                .get(&key)
                .unwrap_or_else(|| panic!("{} is absent from the active descriptor", service.id));
            assert!(
                *method_count > 0,
                "{} {}.{} is an empty marker service",
                service.id,
                service.package,
                service.name
            );
        }
    }

    #[test]
    fn clippy_allowance_is_scoped_to_generated_server_modules() {
        let build_source = include_str!("../build.rs");
        assert!(build_source.contains("server_mod_attribute"));
        assert!(build_source.contains("GENERATED_SERVER_CLIPPY_ALLOW"));
        assert!(build_source.contains("DataPlaneAgentEnvelope.payload"));
        assert!(build_source.contains("clippy::large_enum_variant"));
        assert!(!build_source.contains("crate_attribute"));
        assert!(
            !include_str!("lib.rs").contains(concat!("allow(clippy::", "match_single_binding)"))
        );
    }
}
