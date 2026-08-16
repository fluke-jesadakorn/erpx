#![doc = "Versioned ERPX Tonic contracts."]

/// The descriptor set generated from the checked-in protobuf sources. The
/// build script compares it with the approved development baseline.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/erpx_descriptor_set.bin"));

pub const APPROVED_DESCRIPTOR_SHA256: &str =
    include_str!("../descriptor/erpx_descriptor_set.sha256");

pub const ACTIVE_SERVICE_COUNT: usize = 65;
pub const ACTIVE_SERVICE_ID_FIRST: u16 = 101;
pub const ACTIVE_SERVICE_ID_LAST: u16 = 165;
pub const RETIRED_SERVICE_ID_FIRST: u16 = 1;
pub const RETIRED_SERVICE_ID_LAST: u16 = 65;

pub mod erpx {
    pub mod common {
        pub mod v1 {
            tonic::include_proto!("erpx.common.v1");
        }
    }

    pub mod interface {
        pub mod v1 {
            tonic::include_proto!("erpx.interface.v1");
        }
    }

    pub mod identity_access {
        pub mod v1 {
            tonic::include_proto!("erpx.identity_access.v1");
        }
    }
    pub mod platform_administration {
        pub mod v1 {
            tonic::include_proto!("erpx.platform_administration.v1");
        }
        pub mod data_plane_agent {
            pub mod v1 {
                tonic::include_proto!("erpx.platform_administration.data_plane_agent.v1");
            }
        }
    }
    pub mod organization {
        pub mod v1 {
            tonic::include_proto!("erpx.organization.v1");
        }
    }
    pub mod commercial {
        pub mod v1 {
            tonic::include_proto!("erpx.commercial.v1");
        }
    }
    pub mod brand {
        pub mod v1 {
            tonic::include_proto!("erpx.brand.v1");
        }
    }
    pub mod customer_authority {
        pub mod v1 {
            tonic::include_proto!("erpx.customer_authority.v1");
        }
    }
    pub mod customer_360 {
        pub mod v1 {
            tonic::include_proto!("erpx.customer_360.v1");
        }
    }
    pub mod support {
        pub mod v1 {
            tonic::include_proto!("erpx.support.v1");
        }
    }
    pub mod finance {
        pub mod v1 {
            tonic::include_proto!("erpx.finance.v1");
        }
    }
    pub mod sales {
        pub mod v1 {
            tonic::include_proto!("erpx.sales.v1");
        }
    }
    pub mod pos {
        pub mod v1 {
            tonic::include_proto!("erpx.pos.v1");
        }
    }
    pub mod procurement {
        pub mod v1 {
            tonic::include_proto!("erpx.procurement.v1");
        }
    }
    pub mod inventory {
        pub mod v1 {
            tonic::include_proto!("erpx.inventory.v1");
        }
    }
    pub mod assets {
        pub mod v1 {
            tonic::include_proto!("erpx.assets.v1");
        }
    }
    pub mod maintenance {
        pub mod v1 {
            tonic::include_proto!("erpx.maintenance.v1");
        }
    }
    pub mod quality {
        pub mod v1 {
            tonic::include_proto!("erpx.quality.v1");
        }
    }
    pub mod operations {
        pub mod v1 {
            tonic::include_proto!("erpx.operations.v1");
        }
    }
    pub mod people_payroll {
        pub mod v1 {
            tonic::include_proto!("erpx.people_payroll.v1");
        }
    }
    pub mod legal {
        pub mod v1 {
            tonic::include_proto!("erpx.legal.v1");
        }
    }
    pub mod marketing {
        pub mod v1 {
            tonic::include_proto!("erpx.marketing.v1");
        }
    }
    pub mod crm {
        pub mod v1 {
            tonic::include_proto!("erpx.crm.v1");
        }
    }
    pub mod manufacturing {
        pub mod v1 {
            tonic::include_proto!("erpx.manufacturing.v1");
        }
    }
    pub mod planning {
        pub mod v1 {
            tonic::include_proto!("erpx.planning.v1");
        }
    }
    pub mod projects {
        pub mod v1 {
            tonic::include_proto!("erpx.projects.v1");
        }
    }
    pub mod ecommerce {
        pub mod v1 {
            tonic::include_proto!("erpx.ecommerce.v1");
        }
    }
    pub mod talent {
        pub mod v1 {
            tonic::include_proto!("erpx.talent.v1");
        }
    }
    pub mod learning {
        pub mod v1 {
            tonic::include_proto!("erpx.learning.v1");
        }
    }
    pub mod benefits {
        pub mod v1 {
            tonic::include_proto!("erpx.benefits.v1");
        }
    }
    pub mod field_service {
        pub mod v1 {
            tonic::include_proto!("erpx.field_service.v1");
        }
    }
    pub mod collaboration {
        pub mod v1 {
            tonic::include_proto!("erpx.collaboration.v1");
        }
    }
    pub mod agent {
        pub mod v1 {
            tonic::include_proto!("erpx.agent.v1");
        }
    }
    pub mod reference {
        pub mod v1 {
            tonic::include_proto!("erpx.reference.v1");
        }
    }
}

pub mod common {
    pub mod v1 {
        pub use crate::erpx::common::v1::*;
    }

    pub use v1::*;
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
                .all(|(package, _)| package != concat!("erpx.", "platform.v1"))
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
    fn retired_monolith_proto_source_tree_is_absent() {
        let retired = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("proto/erpx/platform")
            .join("v1");
        assert!(
            !retired.exists(),
            "retired monolith proto sources must not remain in the tree"
        );
    }

    #[test]
    fn active_and_retired_service_id_ranges_do_not_overlap() {
        assert_eq!(ACTIVE_SERVICE_ID_LAST - ACTIVE_SERVICE_ID_FIRST + 1, 65);
        assert_eq!(RETIRED_SERVICE_ID_LAST - RETIRED_SERVICE_ID_FIRST + 1, 65);
        assert!(RETIRED_SERVICE_ID_LAST < std::hint::black_box(ACTIVE_SERVICE_ID_FIRST));
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
