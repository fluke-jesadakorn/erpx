use std::{collections::BTreeSet, fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecisionPolicy {
    HumanOnly,
    OwnerPolicyControlled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Enforcement {
    DomainProof,
    OwnerPolicy,
}

#[derive(Debug, Clone, Copy)]
struct Classification {
    ingress: &'static str,
    policy: DecisionPolicy,
    enforcement: Enforcement,
}

macro_rules! human_domain {
    ($ingress:literal) => {
        Classification {
            ingress: $ingress,
            policy: DecisionPolicy::HumanOnly,
            enforcement: Enforcement::DomainProof,
        }
    };
}

macro_rules! owner_policy {
    ($ingress:literal) => {
        Classification {
            ingress: $ingress,
            policy: DecisionPolicy::OwnerPolicyControlled,
            enforcement: Enforcement::OwnerPolicy,
        }
    };
}

// This is deliberately explicit. Adding an approval- or lifecycle-shaped RPC
// must update this inventory and choose a policy before CI accepts the API.
const CLASSIFICATIONS: &[Classification] = &[
    human_domain!("agent/service.proto:ApplyFieldSuggestion"),
    human_domain!("agent/service.proto:ApproveAgentVersion"),
    owner_policy!("agent/service.proto:CancelAssignment"),
    human_domain!("agent/service.proto:PublishPrivateSessionMessage"),
    human_domain!("agent/service.proto:RejectFieldSuggestion"),
    human_domain!("benefits/service.proto:PublishPlan"),
    human_domain!("benefits/service.proto:ReleasePayroll"),
    human_domain!("brand/service.proto:PublishDraft"),
    human_domain!("collaboration/service.proto:ConfirmActionCard"),
    owner_policy!("collaboration/service.proto:TransitionThread"),
    human_domain!("commercial/service.proto:ActivateBranchEntitlement"),
    human_domain!("commercial/service.proto:ActivateCompanyEntitlement"),
    owner_policy!("commercial/service.proto:ApplyBillingEvent"),
    owner_policy!("commercial/service.proto:ApplyEntitlementChange"),
    human_domain!("commercial/service.proto:PublishCatalogPlan"),
    owner_policy!("crm/service.proto:AcceptMarketingLead"),
    owner_policy!("crm/service.proto:TransitionOpportunity"),
    human_domain!("ecommerce/service.proto:ApproveChannel"),
    human_domain!("ecommerce/service.proto:ApproveListing"),
    human_domain!("ecommerce/service.proto:PublishListing"),
    human_domain!("ecommerce/service.proto:TransitionChannel"),
    owner_policy!("ecommerce/service.proto:TransitionFulfillment"),
    human_domain!("field_service/service.proto:ActivateAgreement"),
    human_domain!("field_service/service.proto:ReleaseBilling"),
    owner_policy!("finance/service.proto:ApplySubledgerSettlement"),
    human_domain!("finance/service.proto:ClosePeriod"),
    human_domain!("finance/service.proto:PostJournal"),
    human_domain!("finance/service.proto:PostVendorInvoice"),
    human_domain!("finance/service.proto:ReleasePayment"),
    human_domain!("finance/service.proto:ReopenPeriod"),
    human_domain!("identity_access/service.proto:ActivatePrincipal"),
    human_domain!("identity_access/service.proto:ApproveDirectoryIdentityReplacement"),
    human_domain!("identity_access/service.proto:RejectDirectoryIdentityReplacement"),
    human_domain!("identity_access/service.proto:TransitionDirectoryConnectorLifecycle"),
    owner_policy!("inventory/service.proto:ReleaseReservation"),
    human_domain!("learning/service.proto:PublishCourse"),
    human_domain!("legal/service.proto:TransitionContract"),
    owner_policy!("maintenance/service.proto:Cancel"),
    owner_policy!("manufacturing/service.proto:ApplyQualityDisposition"),
    human_domain!("manufacturing/service.proto:CloseOrder"),
    human_domain!("manufacturing/service.proto:PublishSpecification"),
    human_domain!("manufacturing/service.proto:ReleaseOrder"),
    human_domain!("marketing/service.proto:RejectLeadCandidate"),
    human_domain!("operations/service.proto:CloseException"),
    human_domain!("operations/service.proto:DecideApproval"),
    human_domain!("operations/service.proto:PublishWorkQueuePolicy"),
    human_domain!("organization/service.proto:ActivateNode"),
    human_domain!("planning/service.proto:ApproveRecommendation"),
    human_domain!("planning/service.proto:RejectRecommendation"),
    human_domain!("planning/service.proto:ReleaseRecommendation"),
    human_domain!("platform_administration/service.proto:ActivateHolding"),
    human_domain!("platform_administration/service.proto:ActivateProjectionRebuild"),
    human_domain!("platform_administration/service.proto:DecideDesiredConfiguration"),
    human_domain!("platform_administration/service.proto:DecideRootAccess"),
    human_domain!("platform_administration/service.proto:PublishCorporateIdentityDraft"),
    human_domain!("platform_administration/service.proto:TransitionConnector"),
    human_domain!("platform_administration/service.proto:TransitionHolding"),
    human_domain!("pos/service.proto:ApproveRefund"),
    owner_policy!("pos/service.proto:CancelRefund"),
    owner_policy!("pos/service.proto:PostRefund"),
    human_domain!("pos/service.proto:RejectRefund"),
    owner_policy!("procurement/service.proto:Cancel"),
    owner_policy!("procurement/service.proto:CancelGoodsReceipt"),
    owner_policy!("procurement/service.proto:CancelRequisition"),
    human_domain!("procurement/service.proto:AcceptGoodsReceipt"),
    human_domain!("procurement/service.proto:ApproveRequisition"),
    human_domain!("procurement/service.proto:RejectGoodsReceipt"),
    human_domain!("procurement/service.proto:RejectRequisition"),
    human_domain!("procurement/service.proto:Release"),
    human_domain!("projects/service.proto:ApproveHandoff"),
    owner_policy!("projects/service.proto:CancelDependency"),
    owner_policy!("projects/service.proto:CancelHandoff"),
    human_domain!("projects/service.proto:DecideDependency"),
    human_domain!("projects/service.proto:TransitionNode"),
    human_domain!("projects/service.proto:TransitionProject"),
    owner_policy!("quality/service.proto:Cancel"),
    human_domain!("quality/service.proto:Release"),
    human_domain!("sales/service.proto:AcceptQuote"),
    owner_policy!("sales/service.proto:CancelSalesOrder"),
    human_domain!("sales/service.proto:ConfirmSalesOrder"),
    human_domain!("sales/service.proto:RejectQuote"),
    human_domain!("support/service.proto:ApproveSession"),
    human_domain!("support/service.proto:TransitionIncident"),
    human_domain!("support/service.proto:TransitionTicket"),
    human_domain!("talent/service.proto:ApproveOffer"),
    human_domain!("talent/service.proto:ApproveProposal"),
    human_domain!("talent/service.proto:ApproveRequisition"),
    human_domain!("talent/service.proto:DecideOffer"),
    human_domain!("talent/service.proto:TransitionRequisition"),
    human_domain!("people_payroll/service.proto:TransitionEmployee"),
    human_domain!("people_payroll/service.proto:TransitionExpenseClaim"),
    human_domain!("people_payroll/service.proto:TransitionLeaveRequest"),
    human_domain!("people_payroll/service.proto:TransitionPayrollRun"),
];

const DECISION_PREFIXES: &[&str] = &[
    "Accept",
    "Activate",
    "Apply",
    "Approve",
    "Cancel",
    "Close",
    "Confirm",
    "Decide",
    "Post",
    "Publish",
    "Reject",
    "Release",
    "Reopen",
    "Transition",
];

#[test]
fn every_decision_shaped_rpc_has_an_explicit_policy_classification() {
    let proto_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/proto/proto/erpx")
        .canonicalize()
        .expect("ERPX proto root");
    let mut discovered = BTreeSet::new();
    collect_decision_rpcs(&proto_root, &proto_root, &mut discovered);

    let classified = CLASSIFICATIONS
        .iter()
        .map(|entry| entry.ingress.to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        discovered, classified,
        "decision-shaped RPC inventory changed; classify the new or renamed ingress before exposing it"
    );
}

#[test]
fn every_human_only_ingress_requires_owner_domain_proof() {
    let mut unique = BTreeSet::new();
    for entry in CLASSIFICATIONS {
        assert!(unique.insert(entry.ingress), "duplicate decision ingress");
        match entry.policy {
            DecisionPolicy::HumanOnly => assert_eq!(entry.enforcement, Enforcement::DomainProof),
            DecisionPolicy::OwnerPolicyControlled => {
                assert_eq!(entry.enforcement, Enforcement::OwnerPolicy)
            }
        }
    }
}

fn collect_decision_rpcs(root: &Path, directory: &Path, output: &mut BTreeSet<String>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .collect::<Result<Vec<_>, _>>()
        .expect("read proto directory entries");
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_decision_rpcs(root, &path, output);
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) != Some("service.proto") {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("service proto beneath root")
            .to_string_lossy()
            .replace('\\', "/");
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for line in source.lines() {
            let Some(signature) = line.trim().strip_prefix("rpc ") else {
                continue;
            };
            let Some((name, _)) = signature.split_once('(') else {
                continue;
            };
            let name = name.trim();
            if DECISION_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                output.insert(format!("{relative}:{name}"));
            }
        }
    }
}
