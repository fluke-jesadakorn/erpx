export type DeliveryStatus =
  | "implemented"
  | "flow_accepted"
  | "sandbox_verified"
  | "live_verified"
  | "deferred";

export type DeliveryStage = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7;

export type CapabilityDelivery = {
  stage: DeliveryStage;
  status: DeliveryStatus;
  ui: string;
  api: string;
  service: string;
  schema: string;
  test: string;
  gap: string;
};

type ModuleDelivery = CapabilityDelivery & {
  overrides?: Record<string, Partial<CapabilityDelivery>>;
};

const preview = (stage: DeliveryStage, gap: string): CapabilityDelivery => ({
  stage,
  status: "deferred",
  ui: "src/app/holdings/[holdingSlug]/modules/[module]/[section] (preview)",
  api: "None",
  service: "None",
  schema: "None",
  test: "tests/ui-registry.test.ts",
  gap,
});

const delivery: Record<string, ModuleDelivery> = {
  legal: {
    stage: 4,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/legal/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/legal/[[...path]]/route.ts",
    service: "src/server/legal/service.ts",
    schema: "migrations/postgres/holding-write",
    test: "tests/legal.test.ts",
    gap: "Complete Company policy/trust/authority review, real portal/email onboarding and business sign-off; integration evidence does not establish legal or production acceptance.",
  },
  brand: {
    stage: 6,
    status: "implemented",
    ui: "src/components/modules/brand-administration.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/branding/[[...path]]/route.ts",
    service: "src/server/branding.ts",
    schema: "migrations/postgres/holding-write/0025_workspace_branding.sql",
    test: "tests/branding.test.ts",
    gap: "Finish the catalog-facing draft, category-lock and rollback journeys.",
  },
  commercial: {
    stage: 6,
    status: "sandbox_verified",
    ui: "src/app/platform/billing/page.tsx",
    api: "src/app/api/v1/platform/billing/[[...path]]/route.ts",
    service: "src/server/billing.ts",
    schema: "migrations/postgres/control/0009_billing.sql",
    test: "tests/billing.test.ts",
    gap: "Sandbox offering only; live billing, tax, refunds and access restriction remain deferred.",
  },
  identity_access: {
    stage: 2,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/iam/[[...section]]/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/iam/[[...path]]/route.ts",
    service: "src/server/iam-service.ts",
    schema:
      "migrations/postgres/holding-write/0029_people_directory_operations.sql",
    test: "tests/iam.test.ts",
    gap: "Complete recovery, invitation delivery and production provider acceptance.",
    overrides: {
      assignments: { status: "flow_accepted" },
      "feature-people-principals": { status: "flow_accepted" },
      "feature-roles-permissions": { status: "flow_accepted" },
      "feature-directory-connectors": {
        status: "live_verified",
        gap: "UCS/Samba lab writes are verified; public connector activation and Windows endpoint enforcement remain gated.",
      },
    },
  },
  organization: {
    stage: 2,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/people/organization/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/people/[...path]/route.ts",
    service: "src/server/organization.ts",
    schema: "migrations/postgres/holding-write/0026_people_lifecycle.sql",
    test: "tests/organization.test.ts",
    gap: "Finish explicit sharing and full node lifecycle acceptance beyond the People workspace.",
  },
  platform_administration: {
    stage: 6,
    status: "implemented",
    ui: "src/app/platform/[destination]/page.tsx",
    api: "src/app/api/v1/onboarding/route.ts",
    service: "src/server/administration.ts",
    schema: "migrations/postgres/control",
    test: "tests/native-recovery.test.ts",
    gap: "Replace remaining sample consoles with operational provisioning, migration, projection, temporary-access and recovery views.",
  },
  customer_360: {
    stage: 3,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/commerce/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
    service: "src/server/commerce.ts",
    schema: "migrations/postgres/holding-write/0001_baseline.sql",
    test: "tests/business-commands.test.ts",
    gap: "Add authorized service timeline, customer notes and a federated customer projection.",
    overrides: {
      "service-timeline": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/customers/[id]/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/customers/route.ts",
        service: "src/server/customer-timeline.ts",
        schema:
          "migrations/postgres/holding-write/0215_crm_leads_opportunities.sql",
        test: "tests/customer-timeline.test.ts",
        gap: "Scoped source projection for sales/fulfillment/returns/contact/follow-up/qualified CRM, explicit field allowlist and pagination. Support/Field Service/Legal/Finance integrations, customer identity and Portal authority, full multi-role/business acceptance remain.",
      },
      "feature-customer-notes": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/commerce/orders/[id]/notes/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/customer-notes/route.ts",
        service: "src/server/customer-notes.ts",
        schema:
          "migrations/postgres/holding-write/0212_customer_contact_notes.sql",
        test: "tests/customer-notes.test.ts",
        gap: "Internal order/quotation contact notes with source scope, immutable correction/archive history, authority and concurrency guards. Standalone customer notes, customer merge/consent, cross-domain timeline, multi-role browser and business acceptance remain.",
      },
    },
  },
  customer_authority: {
    ...preview(
      3,
      "Implement explicit customer/object authority bindings, merge review, revocation and access audit.",
    ),
    overrides: {
      "duplicate-review": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/customer-duplicates/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/customer-duplicates/route.ts",
        service: "src/server/customer-duplicates.ts",
        schema:
          "migrations/postgres/holding-write/0222_customer_duplicate_reviews.sql",
        test: "tests/customer-duplicates.test.ts",
        gap: "Ordered customer pair, independent current reviewer, duplicate/distinct/reopen decisions, canonical selection and immutable history. Transactional merge/repoint/reconciliation, identity changes, browser and business acceptance remain.",
      },
      "feature-identity-changes": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/customer-identity/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/customer-identity/route.ts",
        service: "src/server/customer-identity-changes.ts",
        schema:
          "migrations/postgres/holding-write/0223_customer_identity_changes.sql",
        test: "tests/customer-identity-changes.test.ts",
        gap: "Before/after snapshot, independent current review, reject/resubmit, optimistic customer-version guard and atomic name/tax update with immutable history. Address/contact identity, consent, external registry checks, browser and business acceptance remain.",
      },
      "feature-authority-bindings": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/customer-access/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/customer-access/route.ts",
        service: "src/server/customer-access.ts",
        schema: "migrations/postgres/holding-write/0221_customer_access.sql",
        test: "tests/customer-access.test.ts",
        gap: "Source-bound email authority with independent review, expiry, immediate revocation, first verified-account binding and allowlisted customer projection. Consent preferences, delegated customer administrators, multi-source grants, notifications and business acceptance remain.",
      },
      "access-history": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/customer-access/[id]/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/customer-access/route.ts",
        service: "src/server/customer-portal.ts",
        schema: "migrations/postgres/holding-write/0221_customer_access.sql",
        test: "tests/customer-access.test.ts",
        gap: "Immutable request/review/revoke events and per-view portal audit are available to scoped staff. Search/export/retention policy and privacy-officer acceptance remain.",
      },
    },
  },
  support: {
    ...preview(
      3,
      "Implement SLA clocks, incidents, support sessions, scoped capabilities and private attachments.",
    ),
    overrides: {
      "feature-support-sessions": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/support/sessions/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/support/sessions/route.ts",
        service: "src/server/support-sessions.ts",
        schema: "migrations/postgres/holding-write/0224_support_sessions.sql",
        test: "tests/support-sessions.test.ts",
        gap: "Ticket-bound self-request, independent current manager review, eight-hour ceiling, expiry, immediate revoke, current target eligibility and immutable history are implemented with Work/Inbox. Customer-originated authorization, remote-control tooling, Calendar, browser and business acceptance remain.",
      },
      "feature-capabilities": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/support/sessions/[id]/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/support/sessions/route.ts",
        service: "src/server/support-sessions.ts",
        schema: "migrations/postgres/holding-write/0224_support_sessions.sql",
        test: "tests/support-sessions.test.ts",
        gap: "Explicit allowlisted customer-order context and ticket-evidence metadata grants have issue/reissue/revoke, session cascade, optimistic versions and enforcement on every read. File bytes, write capabilities, Portal consent, export/retention, browser and business acceptance remain.",
      },
      "feature-attachments": {
        status: "implemented",
        ui: "src/components/modules/support-evidence-console.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/uploads/route.ts",
        service: "src/server/support-evidence.ts",
        schema: "migrations/postgres/holding-write/0220_support_evidence.sql",
        test: "tests/support-evidence.test.ts",
        gap: "Private ticket upload/download/withdrawal, source versions, current eligibility, immutable history and resolution freeze. Incident files, Portal sharing/revocation, retention/destruction and multi-role customer acceptance remain.",
      },
      incidents: {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/support/incidents/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/support/incidents/route.ts",
        service: "src/server/support-incidents.ts",
        schema: "migrations/postgres/holding-write/0219_support_incidents.sql",
        test: "tests/support-incidents.test.ts",
        gap: "Source-linked incident, eligible owner, immutable event history, link/unlink/relink, investigation, mitigation, independent review/rework/reopen and Work/Inbox/Calendar. Customer tickets stay independent. Cross-scope coordination, specialist dispatch, severity SLA, public status/email, attachments and postmortem remain.",
      },
      "sla-watch": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/support/sla/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/support/sla/route.ts",
        service: "src/server/support-sla.ts",
        schema: "migrations/postgres/holding-write/0217_support_sla.sql",
        test: "tests/support-sla.test.ts",
        gap: "Independent policy review, immutable calendar snapshots, working-time resolution clocks, pause/resume/reopen, durable breach sweep and scoped manager Work/Inbox acknowledgement. First-response targets, default/effective-dated binding, multi-level escalation, acknowledged-owner reassignment, overnight calendars and customer notification/acceptance remain.",
      },
      tickets: {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/support/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/support/route.ts",
        service: "src/server/support.ts",
        schema: "migrations/postgres/holding-write/0216_support_tickets.sql",
        test: "tests/support.test.ts",
        gap: "Source-linked ticket ownership, acceptance, wait/resume, proposed resolution, independently reviewed staff-recorded customer confirmation, reopen/cancel/reassign and Work/Inbox/Calendar. Standalone identity, Portal/public replies, Email, attachments and full customer acceptance remain. Working-time targets and durable breach escalation have a separate SLA owner implementation.",
      },
    },
  },
  finance: {
    stage: 1,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/finance/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/finance/[...path]/route.ts",
    service: "src/server/finance.ts",
    schema: "migrations/postgres/holding-write/0001_baseline.sql",
    test: "tests/finance-money.test.ts",
    gap: "Add opening imports/reconciliation, notes, schedules, multi-currency precision, consolidation and full close acceptance.",
    overrides: {
      journals: { status: "flow_accepted" },
      payables: { status: "flow_accepted" },
      "feature-payments": {
        status: "flow_accepted",
        schema:
          "migrations/postgres/holding-write/0200_finance_payment_corrections.sql",
        test: "tests/finance-payment-corrections.test.ts",
        gap: "Payment settlement and independent correction have acceptance evidence. Paid-return refund obligations, manual instructions and matched bank GL are implemented in src/server/refunds.ts with tests/commerce-returns.test.ts; Invoice-linked supplier batches with independent approval, export and per-item bank/GL settlement are implemented in src/server/supplier-payments.ts with tests/supplier-payments.test.ts. Payroll/batch unification, FX, provider refunds and multi-role browser acceptance remain.",
      },
      "feature-bank-reconciliation": { status: "flow_accepted" },
      "feature-opening-balances": {
        status: "implemented",
        test: "tests/finance-opening-balances.test.ts",
        gap: "Opening import is previewed, revision-bound and auditable; add opening subledger reconciliation and browser acceptance before Flow accepted.",
      },
    },
  },
  pos: {
    stage: 5,
    status: "flow_accepted",
    ui: "src/app/holdings/[holdingSlug]/pos/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/pos/[...path]/route.ts",
    service: "src/server/pos.ts",
    schema: "migrations/postgres/holding-write/0030_pos.sql",
    test: "tests/pos.test.ts",
    gap: "Complete real Stripe/peripheral pilot acceptance and the POS-to-room charge handoff.",
  },
  sales: {
    stage: 1,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/commerce/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
    service: "src/server/commerce.ts",
    schema: "migrations/postgres/holding-write/0001_baseline.sql",
    test: "tests/business-commands.test.ts",
    gap: "Customer returns now split invoice credit and paid-amount refund obligations with independent approval, manual instructions and bank reconciliation. Add price-list/discount policy, quality disposition, customer notification and multi-role browser acceptance.",
    overrides: {
      quotes: { status: "flow_accepted" },
      orders: { status: "flow_accepted", test: "tests/flow-routing.test.ts" },
      fulfillment: { status: "flow_accepted" },
      "feature-cancellations": {
        status: "flow_accepted",
        test: "tests/flow-routing.test.ts",
      },
      "feature-revenue-handoff": {
        status: "flow_accepted",
        schema: "migrations/postgres/holding-write/0201_commerce_returns.sql",
        test: "tests/commerce-returns.test.ts",
      },
    },
  },
  assets: {
    stage: 4,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/assets/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/assets/route.ts",
    service: "src/server/asset-maintenance.ts",
    schema: "migrations/postgres/holding-write/0225_asset_maintenance.sql",
    test: "tests/asset-maintenance.test.ts",
    gap: "Register, scoped placement/custody, condition history, maintenance linkage and independently reviewed retirement are implemented. Depreciation/Finance linkage, serial/attachment evidence, preventive schedules, browser and business acceptance remain.",
  },
  inventory: {
    stage: 1,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/stock-counts/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/stock-counts/[[...path]]/route.ts",
    service: "src/server/inventory-counts.ts",
    schema: "migrations/postgres/holding-write/0001_baseline.sql",
    test: "tests/business-commands.test.ts",
    gap: "Sales and POS now share reservation availability. Add Production/Field Service adapters, lot-level quality disposition, lot/serial, supplier partial receiving and landed-cost workflows. Staged warehouse transport now supports independent dispatch/partial receipt/discrepancy accounting.",
    overrides: {
      availability: { status: "flow_accepted" },
      reservations: {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/commerce/orders/[id]/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
        service: "src/server/commerce.ts",
        schema:
          "migrations/postgres/holding-write/0205_sales_stock_reservations.sql",
        test: "tests/stock-reservations.test.ts",
        gap: "Sales/POS reserve, release, expiry and partial consumption implemented. Production/Field Service adapters, quality disposition, proactive expiry escalation and multi-role browser acceptance remain incomplete.",
      },
      "feature-stock-register": { status: "flow_accepted" },
      "feature-receipts": { status: "flow_accepted" },
      "feature-issues-movements": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/stock-transit/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/stock-transit/[[...path]]/route.ts",
        service: "src/server/inventory-transit.ts",
        schema: "migrations/postgres/holding-write/0207_inventory_transit.sql",
        test: "tests/inventory-transit.test.ts",
        gap: "Direct transfer compatibility plus approved dispatch, in-transit balances, partial receipt and independently posted loss. Lot/serial traceability, intercompany documents, customer POD and multi-role browser/business acceptance remain incomplete.",
      },
    },
  },
  maintenance: {
    stage: 4,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/maintenance/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/maintenance/route.ts",
    service: "src/server/asset-maintenance.ts",
    schema: "migrations/postgres/holding-write/0225_asset_maintenance.sql",
    test: "tests/asset-maintenance.test.ts",
    gap: "Asset-bound work orders support eligible ownership, scheduling/reassignment, downtime, block/resume, evidenced completion/cancellation, resulting condition and immutable history with Work/Inbox. Parts reservation/costs, recurring preventive plans, Calendar, attachments, browser and business acceptance remain.",
  },
  operations: {
    stage: 4,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/staff-tasks/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/staff/[[...path]]/route.ts",
    service: "src/server/staff-tasks.ts",
    schema: "migrations/postgres/holding-write/0001_baseline.sql",
    test: "tests/staff-work.test.ts",
    gap: "Add policy-owned escalation, resolution, exception summaries and queue administration.",
  },
  procurement: {
    stage: 1,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/commerce/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
    service: "src/server/commerce.ts",
    schema: "migrations/postgres/holding-write/0001_baseline.sql",
    test: "tests/business-commands.test.ts",
    gap: "Supplier returns, debit-note GL and versioned supplier delivery commitments are implemented; reviewed supplier invoice price/quantity matching and AP/GL handoff are implemented with original-cost returns. Partial/split requisition allocation, mandatory matching policy, multi-receipt invoices and complete browser/provider acceptance remain.",
    overrides: {
      requisitions: {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/requests/[id]/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/requests/[[...path]]/route.ts",
        service: "src/server/employee-requests.ts",
        schema:
          "migrations/postgres/holding-write/0210_rfq_requisition_links.sql",
        test: "tests/employee-services.test.ts",
        gap: "Approved employee requisition maps every item to RFQ product/quantity and atomically links an independently awarded draft PO. Partial allocations, split awards, UOM conversion and full multi-role browser acceptance remain.",
      },
      "purchase-orders": { status: "flow_accepted" },
      "goods-receipts": { status: "flow_accepted" },
      "feature-supplier-history": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/commerce/orders/[id]/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/supplier-promises/[id]/route.ts",
        service: "src/server/supplier-promises.ts",
        schema: "migrations/postgres/holding-write/0208_supplier_promises.sql",
        test: "tests/supplier-promises.test.ts",
        gap: "Immutable manual supplier commitments, split dates, partial receipts and scoped Work/Inbox/Calendar implemented. RFQ comparison and independent award-to-PO handoff implemented separately. Partial/split requisition allocation, supplier portal/email evidence and performance reporting remain.",
      },
    },
  },
  quality: {
    ...preview(
      4,
      "Whole product/warehouse quarantine is implemented; lot-level inspection, measurements, attachments, scrap/return disposition remain incomplete. Uninspected cancellation requires independent review.",
    ),
    overrides: Object.fromEntries(
      ["inspections", "results", "release-gates", "feature-cancellation"].map(
        (id) => [
          id,
          {
            status: "implemented" as const,
            ui: "src/app/holdings/[holdingSlug]/quality/page.tsx",
            api: "src/app/api/v1/holdings/[holdingSlug]/quality/[[...path]]/route.ts",
            service: "src/server/quality.ts",
            schema:
              "migrations/postgres/holding-write/0213_quality_hold_cancellation.sql",
            test: "tests/quality.test.ts",
            gap: "Whole warehouse/product hold, immutable pass/fail evidence, independent release, uninspected hold cancellation request/reject/approve, and Work/Inbox/Calendar implemented. Lot/serial-level inspection, structured measurements, attachments, scrap/return disposition, escalation and multi-role browser/business acceptance remain incomplete.",
          },
        ],
      ),
    ),
  },
  people_payroll: {
    stage: 2,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/people/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/people/[...path]/route.ts",
    service: "src/server/people-payroll.ts",
    schema: "migrations/postgres/holding-write/0026_people_lifecycle.sql",
    test: "tests/people-lifecycle.test.ts",
    gap: "Versioned payroll inputs, configurable overtime, calendar-day proration, deductions and immutable review exports are implemented; add advances, final-pay preparation and post-approval correction. Country packs remain deferred.",
    overrides: {
      employees: { status: "flow_accepted" },
      "feature-leave-requests": { status: "flow_accepted" },
      "feature-expenses": { status: "flow_accepted" },
      "feature-payroll-runs": {
        status: "implemented",
        schema:
          "migrations/postgres/holding-write/0202_payroll_core_configuration.sql",
        test: "tests/payroll-core.test.ts",
      },
    },
  },
  crm: {
    stage: 3,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/commerce/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
    service: "src/server/commerce.ts",
    schema: "migrations/postgres/holding-write/0001_baseline.sql",
    test: "tests/business-commands.test.ts",
    gap: "Lead qualification, scoped opportunity pipeline and source-approved quotation/order handoff implemented; customer identity/consent, direct lead activities, SLA escalation and full S01 acceptance remain.",
    overrides: {
      activities: {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/customer-followups/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/customer-followups/route.ts",
        service: "src/server/customer-followups.ts",
        schema: "migrations/postgres/holding-write/0214_customer_followups.sql",
        test: "tests/customer-followups.test.ts",
        gap: "Source-order follow-ups with eligible owner, due date, audited reassignment/cancellation/completion and Work/Inbox/Calendar. Lead/opportunity activities, SLA escalation, communications provider delivery and multi-role browser/business acceptance remain.",
      },
      "feature-leads": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/crm/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/crm/route.ts",
        service: "src/server/crm.ts",
        schema:
          "migrations/postgres/holding-write/0215_crm_leads_opportunities.sql",
        test: "tests/crm.test.ts",
        gap: "Scoped lead qualification/disqualification/reopen, eligible owner reassignment and immutable brief/history with Work/Inbox/Calendar. Deduplication, customer identity/consent, direct lead activities, SLA/escalation and complete browser/business acceptance remain.",
      },
      "feature-opportunities": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/crm/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/crm/route.ts",
        service: "src/server/crm.ts",
        schema:
          "migrations/postgres/holding-write/0215_crm_leads_opportunities.sql",
        test: "tests/crm.test.ts",
        gap: "Versioned pipeline, approved quote linkage, confirmed source order before win, loss/reopen and exact currency-separated weighted forecast. Contract/brief propagation, forecast policy snapshots, multi-currency precision and full browser/business acceptance remain.",
      },
    },
  },
  marketing: {
    stage: 5,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/marketing/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/marketing/route.ts",
    service: "src/server/marketing.ts",
    schema: "migrations/postgres/holding-write/0232_marketing.sql",
    test: "tests/marketing.test.ts",
    gap: "Independently activated forms/content/domain/provider assets, consented attributed lead candidates, engagement evidence and idempotent CRM owner-service handoff are implemented. Public form hosting, provider delivery/webhooks, campaign analytics, browser and business acceptance remain.",
  },
  manufacturing: {
    stage: 4,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/manufacturing/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/production/route.ts",
    service: "src/server/production.ts",
    schema: "migrations/postgres/holding-write/0233_planning_manufacturing.sql",
    test: "tests/production.test.ts",
    gap: "Versioned BOM/formula, planning-released orders, shared material reservations, actual issue/cost, output/scrap, independent QC, finished stock and genealogy are implemented. Multi-operation routing, lot/serial genealogy, machine downtime, WIP/GL posting, browser and business acceptance remain.",
  },
  planning: {
    stage: 4,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/planning/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/production/route.ts",
    service: "src/server/production.ts",
    schema: "migrations/postgres/holding-write/0233_planning_manufacturing.sql",
    test: "tests/production.test.ts",
    gap: "Source-versioned demand scenarios calculate shared-stock shortages, capacity-derived feasible dates and independently reviewed production recommendations. Incoming PO/order demand, resource calendars, alternate plans, browser and business acceptance remain.",
  },
  projects: {
    stage: 4,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/projects/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/staff/[[...path]]/route.ts",
    service: "src/server/staff-tasks.ts",
    schema: "migrations/postgres/holding-write/0001_baseline.sql",
    test: "tests/staff-work.test.ts",
    gap: "Complete WBS dependencies, resource/time/cost ownership and project graph acceptance.",
  },
  ecommerce: {
    stage: 5,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/ecommerce/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/ecommerce/route.ts",
    service: "src/server/ecommerce.ts",
    schema: "migrations/postgres/holding-write/0234_ecommerce.sql",
    test: "tests/ecommerce.test.ts",
    gap: "Independently reviewed channels and listings, version-locked carts, idempotent checkout-to-Sales handoff, shared stock reservations, owner fulfillment links and invoice-gated returns are implemented. Live storefront/payment provider, browser and business acceptance remain.",
  },
  benefits: {
    stage: 6,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/benefits/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/benefits/route.ts",
    service: "src/server/benefits.ts",
    schema: "migrations/postgres/holding-write/0229_benefits.sql",
    test: "tests/benefits.test.ts",
    gap: "Reviewed plans and windows, employee dependents/elections, provider acknowledgement and immutable payroll inputs are implemented. Live provider files, enrollment migration, Thai expert review, multi-role browser and business acceptance remain.",
  },
  learning: {
    stage: 6,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/learning/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/learning/route.ts",
    service: "src/server/learning.ts",
    schema: "migrations/postgres/holding-write/0228_learning.sql",
    test: "tests/learning.test.ts",
    gap: "Reviewed course publication, current-worker assignments, learner attendance/results, evidenced completion or reviewed waiver, Work/Inbox and immutable eligibility handoff are implemented. Provider import, certificates/expiry, benefits consumption, Calendar, browser and business acceptance remain.",
  },
  talent: {
    stage: 6,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/talent/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/talent/route.ts",
    service: "src/server/talent.ts",
    schema: "migrations/postgres/holding-write/0230_talent.sql",
    test: "tests/talent.test.ts",
    gap: "Reviewed requisitions, consented private candidates, source-versioned applications, interview proposals, independently approved offers and immutable hire handoffs are implemented. Candidate portal/signature, background checks, handoff consumption, retention erasure, browser and business acceptance remain.",
  },
  field_service: {
    stage: 4,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/field-service/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/field-service/route.ts",
    service: "src/server/field-service.ts",
    schema: "migrations/postgres/holding-write/0226_field_service.sql",
    test: "tests/field-service.test.ts",
    gap: "Agreement activation, ticket-bound service orders, current technician dispatch, shared part reservation/issue, site exceptions, customer-confirmed completion and independent billing release are implemented with Work/Inbox. Invoice creation, warranty pricing, Calendar, attachment bytes, browser and business acceptance remain.",
  },
  agent: {
    stage: 7,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/agents/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/agents/route.ts",
    service: "src/server/agent-governance.ts",
    schema: "migrations/postgres/holding-write/0231_agent_governance.sql",
    test: "tests/agent-governance.test.ts",
    gap: "Reviewed catalogs/versions, user or pool assignments, private scoped sessions and immutable invocation evidence wrap the read-only assistant. Expand source adapters, evaluation/monitoring, retention/export, browser and business acceptance.",
    overrides: {
      "feature-field-suggestions": {
        status: "implemented",
        gap: "Expand authorized source adapters to every completed domain.",
      },
    },
  },
  collaboration: {
    stage: 2,
    status: "implemented",
    ui: "src/app/holdings/[holdingSlug]/feed/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/feed/route.ts",
    service: "src/server/social.ts",
    schema: "migrations/postgres/holding-write/0001_baseline.sql",
    test: "tests/social-beta.test.ts",
    gap: "Complete moderation, typed record relations/action cards, retention/export and attachment coverage.",
    overrides: {
      "feature-comments-moderation": { status: "flow_accepted" },
      "feature-following-reactions": { status: "flow_accepted" },
      "feature-topics-feed": { status: "flow_accepted" },
      "feature-attachments": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/inbox/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/messaging/[[...path]]/route.ts",
        service: "src/server/messaging-files.ts",
        schema: "migrations/postgres/holding-write/0001_baseline.sql",
        test: "tests/messaging.test.ts",
        gap: "Private conversation file upload, draft isolation, one-time binding, current room membership download and membership revocation are implemented. Retention/export, legal hold and full browser/business acceptance remain.",
      },
      "feature-action-cards": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/collaboration/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/collaboration/route.ts",
        service: "src/server/collaboration-work.ts",
        schema: "migrations/postgres/holding-write/0227_collaboration_work.sql",
        test: "tests/collaboration-work.test.ts",
        gap: "Source-versioned action cards, current eligible owner, Work/Inbox, evidence completion and reviewed reassignment/cancellation are implemented for four owner domains. Expand source adapters, Calendar, retention/export and browser/business acceptance.",
      },
      "feature-record-relations": {
        status: "implemented",
        ui: "src/app/holdings/[holdingSlug]/collaboration/page.tsx",
        api: "src/app/api/v1/holdings/[holdingSlug]/collaboration/route.ts",
        service: "src/server/collaboration-work.ts",
        schema: "migrations/postgres/holding-write/0227_collaboration_work.sql",
        test: "tests/collaboration-work.test.ts",
        gap: "Typed version snapshots, symmetric duplicate prevention, permission-filtered backlinks, removal evidence and immutable history are implemented for Commerce, Support, Field Service and staff tasks. Expand adapters and add dead-link/retention acceptance.",
      },
    },
  },
  pms: {
    stage: 5,
    status: "flow_accepted",
    ui: "src/app/holdings/[holdingSlug]/pms/[[...section]]/page.tsx",
    api: "src/app/api/v1/holdings/[holdingSlug]/pms/route.ts",
    service: "src/server/pms.ts",
    schema: "migrations/postgres/holding-write/0031_pms.sql",
    test: "tests/pms.test.ts",
    gap: "Complete pilot setup, POS room-charge handoff, online payment and separately gated OTA adapter acceptance.",
  },
};

export function capabilityDelivery(
  moduleId: string,
  capabilityId: string,
): CapabilityDelivery | undefined {
  const base = delivery[moduleId];
  if (!base) return undefined;
  const defaults: CapabilityDelivery = {
    stage: base.stage,
    status: base.status,
    ui: base.ui,
    api: base.api,
    service: base.service,
    schema: base.schema,
    test: base.test,
    gap: base.gap,
  };
  return { ...defaults, ...base.overrides?.[capabilityId] };
}

export function deliveryModules() {
  return Object.keys(delivery);
}
