import "server-only";
import type { ReactNode } from "react";
import type { RecordKind } from "@erpx/feature-records/lib/record-drawer";
type OwnerPage = (props: {
    params: Promise<{
        holdingSlug: string;
        id: string;
    }>;
    searchParams: Promise<Record<string, string>>;
}) => ReactNode | Promise<ReactNode>;
export const recordOwnerPages: Partial<Record<RecordKind, () => Promise<{
    default: OwnerPage;
}>>> = {
    finance_period: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/close/screen"),
    order: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/commerce/orders/[id]/screen"),
    invoice: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/invoices/[id]/screen"),
    journal: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/journals/[id]/screen"),
    payment: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/payments/[id]/screen"),
    refund: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/refunds/[id]/screen"),
    supplier_batch: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/supplier-batches/[id]/screen"),
    finance_asset: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/assets/[id]/screen"),
    tax_document: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/thai-documents/[id]/screen"),
    vat_entry: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/tax/entries/[id]/screen"),
    vat_correction: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/tax/corrections/[id]/screen"),
    vat_package: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/tax/packages/[id]/screen"),
    statement_package: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/statement-packages/[id]/screen"),
    landed_cost: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/landed-costs/[id]/screen"),
    request: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/requests/[id]/screen"),
    expense: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/expenses/[id]/screen"),
    leave: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/tasks/people_payroll/[id]/screen"),
    staff_task: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/staff-tasks/[id]/screen"),
    stock_count: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/stock-counts/[id]/screen"),
    stock_transfer: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/stock-transfers/[id]/screen"),
    stock_transit: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/stock-transit/[id]/screen"),
    rfq: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/procurement-rfqs/[id]/screen"),
    supplier_bill: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/supplier-bills/[id]/screen"),
    customer: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/customers/[id]/screen"),
    customer_identity: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/customer-identity/[id]/screen"),
    customer_duplicate: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/customer-duplicates/[id]/screen"),
    customer_access: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/customer-access/[id]/screen"),
    customer_followup: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/customer-followups/[id]/screen"),
    crm_lead: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/crm/[id]/screen"),
    marketing_candidate: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/marketing/[id]/screen"),
    support_ticket: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/support/[id]/screen"),
    support_incident: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/support/incidents/[id]/screen"),
    support_session: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/support/sessions/[id]/screen"),
    field_service: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/field-service/[id]/screen"),
    asset: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/assets/[id]/screen"),
    maintenance: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/maintenance/[id]/screen"),
    maintenance_plan: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/maintenance/plans/[id]/screen"),
    quality_hold: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/quality/[id]/screen"),
    manufacturing_order: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/manufacturing/[id]/screen"),
    ecommerce_checkout: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/ecommerce/[id]/screen"),
    person: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/[id]/screen"),
    payroll_run: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/payroll/runs/[id]/screen"),
    payslip: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/payslips/[id]/screen"),
    overtime: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/overtime/[id]/screen"),
    clearance: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/clearance/[id]/screen"),
    learning_assignment: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/learning/[id]/screen"),
    benefit_enrollment: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/benefits/[id]/screen"),
    talent_application: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/talent/[id]/screen"),
    project: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/projects/[id]/screen"),
    routine_template: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/routine-templates/[id]/screen"),
    handbook: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/handbook/[id]/screen"),
    legal: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/legal/screen"),
    pms_document: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/pms/documents/[id]/standard-document"),
    operation: () => import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/operations/[id]/screen"),
};
