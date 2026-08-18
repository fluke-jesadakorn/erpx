#![doc = "Versioned ERPX Tonic contracts."]

/// The descriptor set generated from the checked-in protobuf sources. The
/// build script compares it with the approved development baseline.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/erpx_descriptor_set.bin"));

pub const APPROVED_DESCRIPTOR_SHA256: &str =
    include_str!("../descriptor/erpx_descriptor_set.sha256");

pub const ACTIVE_SERVICE_COUNT: usize = 65;
pub const ACTIVE_SERVICE_ID_FIRST: u16 = 201;
pub const ACTIVE_SERVICE_ID_LAST: u16 = 265;
pub const RETIRED_SERVICE_ID_FIRST: u16 = 101;
pub const RETIRED_SERVICE_ID_LAST: u16 = 165;

pub mod erpx {
    pub mod common {
        pub mod v2 {
            tonic::include_proto!("erpx.common.v2");
        }
    }

    pub mod interface {
        pub mod v2 {
            tonic::include_proto!("erpx.interface.v2");
        }
    }

    pub mod identity_access {
        pub mod v2 {
            tonic::include_proto!("erpx.identity_access.v2");
        }
    }
    pub mod platform_administration {
        pub mod v2 {
            tonic::include_proto!("erpx.platform_administration.v2");
        }
        pub mod data_plane_agent {
            pub mod v2 {
                tonic::include_proto!("erpx.platform_administration.data_plane_agent.v2");
            }
        }
    }
    pub mod organization {
        pub mod v2 {
            tonic::include_proto!("erpx.organization.v2");
        }
    }
    pub mod commercial {
        pub mod v2 {
            tonic::include_proto!("erpx.commercial.v2");
        }
    }
    pub mod brand {
        pub mod v2 {
            tonic::include_proto!("erpx.brand.v2");
        }
    }
    pub mod customer_authority {
        pub mod v2 {
            tonic::include_proto!("erpx.customer_authority.v2");
        }
    }
    pub mod customer_360 {
        pub mod v2 {
            tonic::include_proto!("erpx.customer_360.v2");
        }
    }
    pub mod support {
        pub mod v2 {
            tonic::include_proto!("erpx.support.v2");
        }
    }
    pub mod finance {
        pub mod v2 {
            tonic::include_proto!("erpx.finance.v2");
        }
    }
    pub mod sales {
        pub mod v2 {
            tonic::include_proto!("erpx.sales.v2");
        }
    }
    pub mod pos {
        pub mod v2 {
            tonic::include_proto!("erpx.pos.v2");
        }
    }
    pub mod procurement {
        pub mod v2 {
            tonic::include_proto!("erpx.procurement.v2");
        }
    }
    pub mod inventory {
        pub mod v2 {
            tonic::include_proto!("erpx.inventory.v2");
        }
    }
    pub mod assets {
        pub mod v2 {
            tonic::include_proto!("erpx.assets.v2");
        }
    }
    pub mod maintenance {
        pub mod v2 {
            tonic::include_proto!("erpx.maintenance.v2");
        }
    }
    pub mod quality {
        pub mod v2 {
            tonic::include_proto!("erpx.quality.v2");
        }
    }
    pub mod operations {
        pub mod v2 {
            tonic::include_proto!("erpx.operations.v2");
        }
    }
    pub mod people_payroll {
        pub mod v2 {
            tonic::include_proto!("erpx.people_payroll.v2");
        }
    }
    pub mod legal {
        pub mod v2 {
            tonic::include_proto!("erpx.legal.v2");
        }
    }
    pub mod marketing {
        pub mod v2 {
            tonic::include_proto!("erpx.marketing.v2");
        }
    }
    pub mod crm {
        pub mod v2 {
            tonic::include_proto!("erpx.crm.v2");
        }
    }
    pub mod manufacturing {
        pub mod v2 {
            tonic::include_proto!("erpx.manufacturing.v2");
        }
    }
    pub mod planning {
        pub mod v2 {
            tonic::include_proto!("erpx.planning.v2");
        }
    }
    pub mod projects {
        pub mod v2 {
            tonic::include_proto!("erpx.projects.v2");
        }
    }
    pub mod ecommerce {
        pub mod v2 {
            tonic::include_proto!("erpx.ecommerce.v2");
        }
    }
    pub mod talent {
        pub mod v2 {
            tonic::include_proto!("erpx.talent.v2");
        }
    }
    pub mod learning {
        pub mod v2 {
            tonic::include_proto!("erpx.learning.v2");
        }
    }
    pub mod benefits {
        pub mod v2 {
            tonic::include_proto!("erpx.benefits.v2");
        }
    }
    pub mod field_service {
        pub mod v2 {
            tonic::include_proto!("erpx.field_service.v2");
        }
    }
    pub mod collaboration {
        pub mod v2 {
            tonic::include_proto!("erpx.collaboration.v2");
        }
    }
    pub mod agent {
        pub mod v2 {
            tonic::include_proto!("erpx.agent.v2");
        }
    }
    pub mod reference {
        pub mod v2 {
            tonic::include_proto!("erpx.reference.v2");
        }
    }
}

pub mod common {
    pub mod v2 {
        pub use crate::erpx::common::v2::*;
    }

    pub use v2::*;
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
                .all(|(package, _)| package != concat!("erpx.", "platform.v2"))
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
    fn generated_v1_proto_source_trees_are_absent() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("proto/erpx");
        let retired = root.join("platform").join("v1");
        assert!(
            !retired.exists(),
            "retired monolith proto sources must not remain in the tree"
        );
        assert!(
            !std::fs::read_dir(root)
                .expect("proto root")
                .filter_map(Result::ok)
                .any(|entry| entry.path().join("v1").exists()),
            "generated v1 context modules must not remain in the tree"
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
