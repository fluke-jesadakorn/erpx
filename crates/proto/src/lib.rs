#![doc = "Unversioned ERPX Tonic contracts."]

/// The descriptor set generated from the checked-in protobuf sources. The
/// build script compares it with the approved development baseline.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/erpx_descriptor_set.bin"));

pub const APPROVED_DESCRIPTOR_SHA256: &str =
    include_str!("../descriptor/erpx_descriptor_set.sha256");

pub const ACTIVE_SERVICE_COUNT: usize = 67;
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
    fn collaboration_phase_two_rpcs_are_additive_and_typed() {
        let descriptor = FileDescriptorSet::decode(FILE_DESCRIPTOR_SET)
            .expect("approved descriptor must decode");
        let collaboration = descriptor
            .file
            .iter()
            .find(|file| file.package.as_deref() == Some("erpx.collaboration"))
            .expect("collaboration descriptor");
        let methods = collaboration
            .service
            .iter()
            .flat_map(|service| service.method.iter())
            .filter_map(|method| method.name.as_deref())
            .collect::<BTreeSet<_>>();
        for method in [
            // Phase 1 methods stay present for mixed-version readers.
            "CreateThread",
            "AddComment",
            "GetThread",
            "ListThreadsByRecord",
            "ListRecordRelations",
            "AddRecordRelation",
            "RevokeRecordRelation",
            "GetThreadV2",
            "ListThreadsByRecordV2",
            "StartRecordThreadWithComment",
            "PrepareAttachmentUpload",
            "FinalizeAttachmentUpload",
            "GetAttachmentDownload",
        ] {
            assert!(methods.contains(method), "missing additive RPC {method}");
        }

        let messages = collaboration
            .message_type
            .iter()
            .filter_map(|message| message.name.as_deref().map(|name| (name, message)))
            .collect::<std::collections::BTreeMap<_, _>>();
        let typed_thread = messages
            .get("ThreadProjectionV2")
            .expect("typed thread projection");
        assert!(typed_thread.field.iter().any(|field| {
            field.name.as_deref() == Some("comments")
                && field.type_name.as_deref() == Some(".erpx.collaboration.CommentProjectionV2")
        }));
        assert!(typed_thread.field.iter().any(|field| {
            field.name.as_deref() == Some("retention")
                && field.type_name.as_deref() == Some(".erpx.collaboration.ThreadRetentionPolicyV2")
        }));
        assert!(
            typed_thread
                .field
                .iter()
                .all(|field| field.name.as_deref() != Some("authorized_projection_json_utf8"))
        );
    }

    #[test]
    fn procurement_phase_five_contracts_are_additive_typed_and_source_pinned() {
        let descriptor = FileDescriptorSet::decode(FILE_DESCRIPTOR_SET)
            .expect("approved descriptor must decode");
        let procurement = descriptor
            .file
            .iter()
            .find(|file| file.package.as_deref() == Some("erpx.procurement"))
            .expect("procurement descriptor");
        let methods = procurement
            .service
            .iter()
            .flat_map(|service| service.method.iter())
            .filter_map(|method| method.name.as_deref())
            .collect::<BTreeSet<_>>();
        for method in [
            "Create",
            "Release",
            "Cancel",
            "Get",
            "List",
            "CreateRequisition",
            "SubmitRequisition",
            "ApproveRequisition",
            "RejectRequisition",
            "CancelRequisition",
            "CreateGoodsReceipt",
            "AcceptGoodsReceipt",
            "RejectGoodsReceipt",
            "CancelGoodsReceipt",
            "GetRequisition",
            "GetGoodsReceipt",
        ] {
            assert!(methods.contains(method), "missing additive RPC {method}");
        }

        let messages = procurement
            .message_type
            .iter()
            .filter_map(|message| message.name.as_deref().map(|name| (name, message)))
            .collect::<std::collections::BTreeMap<_, _>>();
        let create_order = messages
            .get("CreatePurchaseOrderRequest")
            .expect("purchase-order create request");
        assert!(create_order.field.iter().any(|field| {
            field.name.as_deref() == Some("source_requisition")
                && field.type_name.as_deref() == Some(".erpx.procurement.RecordVersionRef")
        }));
        assert!(create_order.field.iter().any(|field| {
            field.name.as_deref() == Some("contract") && field.proto3_optional.unwrap_or(false)
        }));

        let receipt_line = messages
            .get("GoodsReceiptLineRecord")
            .expect("typed goods-receipt line");
        for field in [
            "line_id",
            "purchase_order_line_id",
            "inventory_stock_item_id",
            "received_quantity",
        ] {
            assert!(
                receipt_line
                    .field
                    .iter()
                    .any(|candidate| candidate.name.as_deref() == Some(field)),
                "goods receipt line is missing {field}"
            );
        }

        let capabilities = messages
            .get("ProcurementOwnerCapabilities")
            .expect("typed Procurement owner capabilities");
        for field in [
            "source_version",
            "approve_requisition",
            "release_purchase_order",
            "accept_goods_receipt",
            "evaluated_at",
        ] {
            assert!(
                capabilities
                    .field
                    .iter()
                    .any(|candidate| { candidate.name.as_deref() == Some(field) })
            );
        }
        for response in [
            "GetPurchaseOrderResponse",
            "GetPurchaseRequisitionResponse",
            "GetGoodsReceiptResponse",
        ] {
            assert!(messages[response].field.iter().any(|field| {
                field.name.as_deref() == Some("owner_capabilities")
                    && field.type_name.as_deref()
                        == Some(".erpx.procurement.ProcurementOwnerCapabilities")
            }));
        }
    }

    #[test]
    fn finance_payables_contracts_are_typed_exact_versioned_and_distinct_from_ar() {
        let descriptor = FileDescriptorSet::decode(FILE_DESCRIPTOR_SET)
            .expect("approved descriptor must decode");
        let finance = descriptor
            .file
            .iter()
            .find(|file| file.package.as_deref() == Some("erpx.finance"))
            .expect("finance descriptor");
        let methods = finance
            .service
            .iter()
            .flat_map(|service| service.method.iter())
            .filter_map(|method| method.name.as_deref())
            .collect::<BTreeSet<_>>();
        for method in [
            "CreateVendorInvoice",
            "PostVendorInvoice",
            "CreatePayment",
            "ReleasePayment",
            "GetVendorInvoice",
            "GetPayment",
        ] {
            assert!(methods.contains(method), "missing Finance AP RPC {method}");
        }

        let messages = finance
            .message_type
            .iter()
            .filter_map(|message| message.name.as_deref().map(|name| (name, message)))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!(messages.contains_key("FinanceVendorInvoiceRecord"));
        assert!(!messages.contains_key("FinanceInvoiceRecord"));

        let versioned = messages
            .get("FinanceVersionedRecordRef")
            .expect("exact-version Finance reference");
        for field in [
            "context",
            "aggregate_kind",
            "aggregate_id",
            "aggregate_version",
        ] {
            assert!(
                versioned
                    .field
                    .iter()
                    .any(|candidate| candidate.name.as_deref() == Some(field)),
                "Finance exact-version reference is missing {field}"
            );
        }

        let vendor_invoice = messages
            .get("FinanceVendorInvoiceRecord")
            .expect("typed vendor invoice record");
        for (field_name, field_type) in [
            ("purchase_order", ".erpx.finance.FinanceVersionedRecordRef"),
            ("receipt", ".erpx.finance.FinanceVersionedRecordRef"),
            ("tolerance", ".erpx.finance.FinanceThreeWayMatchTolerance"),
            ("total_amount", ".erpx.finance.FinanceMoneyAmount"),
        ] {
            assert!(vendor_invoice.field.iter().any(|field| {
                field.name.as_deref() == Some(field_name)
                    && field.type_name.as_deref() == Some(field_type)
            }));
        }
        assert!(vendor_invoice.field.iter().any(|field| {
            field.name.as_deref() == Some("payable_id") && field.proto3_optional.unwrap_or(false)
        }));

        let payment = messages
            .get("FinancePaymentRecord")
            .expect("typed payment record");
        assert!(payment.field.iter().any(|field| {
            field.name.as_deref() == Some("allocations")
                && field.type_name.as_deref() == Some(".erpx.finance.FinancePaymentAllocation")
        }));

        let capabilities = messages
            .get("FinancePayablesOwnerCapabilities")
            .expect("typed Finance owner capabilities");
        for field in [
            "source_version",
            "post_vendor_invoice",
            "release_payment",
            "evaluated_at",
        ] {
            assert!(
                capabilities
                    .field
                    .iter()
                    .any(|candidate| { candidate.name.as_deref() == Some(field) })
            );
        }
        for response in [
            "GetFinanceVendorInvoiceResponse",
            "GetFinancePaymentResponse",
        ] {
            assert!(messages[response].field.iter().any(|field| {
                field.name.as_deref() == Some("owner_capabilities")
                    && field.type_name.as_deref()
                        == Some(".erpx.finance.FinancePayablesOwnerCapabilities")
            }));
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
