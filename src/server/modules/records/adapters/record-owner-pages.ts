import "server-only";
import type { ReactNode } from "react";
import type { RecordKind } from "@/lib/records/record-drawer";
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
    finance_period: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/close/screen"),
    order: () => import("@/app/holdings/[holdingSlug]/_presenters/commerce/orders/[id]/screen"),
    invoice: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/invoices/[id]/screen"),
    journal: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/journals/[id]/screen"),
    payment: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/payments/[id]/screen"),
    refund: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/refunds/[id]/screen"),
    supplier_batch: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/supplier-batches/[id]/screen"),
    finance_asset: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/assets/[id]/screen"),
    tax_document: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/thai-documents/[id]/screen"),
    vat_entry: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/tax/entries/[id]/screen"),
    vat_correction: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/tax/corrections/[id]/screen"),
    vat_package: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/tax/packages/[id]/screen"),
    statement_package: () => import("@/app/holdings/[holdingSlug]/_presenters/finance/statement-packages/[id]/screen"),
    landed_cost: () => import("@/app/holdings/[holdingSlug]/_presenters/landed-costs/[id]/screen"),
    request: () => import("@/app/holdings/[holdingSlug]/_presenters/requests/[id]/screen"),
    expense: () => import("@/app/holdings/[holdingSlug]/_presenters/expenses/[id]/screen"),
    leave: () => import("@/app/holdings/[holdingSlug]/_presenters/tasks/people_payroll/[id]/screen"),
    staff_task: () => import("@/app/holdings/[holdingSlug]/_presenters/staff-tasks/[id]/screen"),
    stock_count: () => import("@/app/holdings/[holdingSlug]/_presenters/stock-counts/[id]/screen"),
    stock_transfer: () => import("@/app/holdings/[holdingSlug]/_presenters/stock-transfers/[id]/screen"),
    stock_transit: () => import("@/app/holdings/[holdingSlug]/_presenters/stock-transit/[id]/screen"),
    rfq: () => import("@/app/holdings/[holdingSlug]/_presenters/procurement-rfqs/[id]/screen"),
    supplier_bill: () => import("@/app/holdings/[holdingSlug]/_presenters/supplier-bills/[id]/screen"),
    customer: () => import("@/app/holdings/[holdingSlug]/_presenters/customers/[id]/screen"),
    customer_identity: () => import("@/app/holdings/[holdingSlug]/_presenters/customer-identity/[id]/screen"),
    customer_duplicate: () => import("@/app/holdings/[holdingSlug]/_presenters/customer-duplicates/[id]/screen"),
    customer_access: () => import("@/app/holdings/[holdingSlug]/_presenters/customer-access/[id]/screen"),
    customer_followup: () => import("@/app/holdings/[holdingSlug]/_presenters/customer-followups/[id]/screen"),
    crm_lead: () => import("@/app/holdings/[holdingSlug]/_presenters/crm/[id]/screen"),
    marketing_candidate: () => import("@/app/holdings/[holdingSlug]/_presenters/marketing/[id]/screen"),
    support_ticket: () => import("@/app/holdings/[holdingSlug]/_presenters/support/[id]/screen"),
    support_incident: () => import("@/app/holdings/[holdingSlug]/_presenters/support/incidents/[id]/screen"),
    support_session: () => import("@/app/holdings/[holdingSlug]/_presenters/support/sessions/[id]/screen"),
    field_service: () => import("@/app/holdings/[holdingSlug]/_presenters/field-service/[id]/screen"),
    asset: () => import("@/app/holdings/[holdingSlug]/_presenters/assets/[id]/screen"),
    maintenance: () => import("@/app/holdings/[holdingSlug]/_presenters/maintenance/[id]/screen"),
    maintenance_plan: () => import("@/app/holdings/[holdingSlug]/_presenters/maintenance/plans/[id]/screen"),
    quality_hold: () => import("@/app/holdings/[holdingSlug]/_presenters/quality/[id]/screen"),
    manufacturing_order: () => import("@/app/holdings/[holdingSlug]/_presenters/manufacturing/[id]/screen"),
    ecommerce_checkout: () => import("@/app/holdings/[holdingSlug]/_presenters/ecommerce/[id]/screen"),
    person: () => import("@/app/holdings/[holdingSlug]/_presenters/people/[id]/screen"),
    payroll_run: () => import("@/app/holdings/[holdingSlug]/_presenters/people/payroll/runs/[id]/screen"),
    payslip: () => import("@/app/holdings/[holdingSlug]/_presenters/people/payslips/[id]/screen"),
    overtime: () => import("@/app/holdings/[holdingSlug]/_presenters/overtime/[id]/screen"),
    clearance: () => import("@/app/holdings/[holdingSlug]/_presenters/people/clearance/[id]/screen"),
    learning_assignment: () => import("@/app/holdings/[holdingSlug]/_presenters/learning/[id]/screen"),
    benefit_enrollment: () => import("@/app/holdings/[holdingSlug]/_presenters/benefits/[id]/screen"),
    talent_application: () => import("@/app/holdings/[holdingSlug]/_presenters/talent/[id]/screen"),
    project: () => import("@/app/holdings/[holdingSlug]/_presenters/projects/[id]/screen"),
    routine_template: () => import("@/app/holdings/[holdingSlug]/_presenters/routine-templates/[id]/screen"),
    handbook: () => import("@/app/holdings/[holdingSlug]/_presenters/handbook/[id]/screen"),
    legal: () => import("@/app/holdings/[holdingSlug]/_presenters/legal/screen"),
    pms_document: () => import("@/app/holdings/[holdingSlug]/_presenters/pms/documents/[id]/standard-document"),
    operation: () => import("@/app/holdings/[holdingSlug]/_presenters/operations/[id]/screen"),
};
