import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { sourceRecordLabel } from "@erpx/feature-records/lib/source-record";
import { BusinessRuleError } from '@erpx/server-shared/index';
import { staffNotificationSourceTypes, type StaffNotification } from '@erpx/server-collaboration/domain/staff-notifications';
import type { ListPage } from '@erpx/server-work_projection/public/list-page';
type Tail<A extends unknown[]> = A extends [
    unknown,
    ...infer Rest
] ? Rest : never;
export interface StaffNotificationPorts {
    handbookNotificationTarget(actor:WorkContext,id:string):Promise<{href:string;version:number}>;
    financeNotificationTarget(actor:WorkContext,kind:"invoice"|"journal",id:string):Promise<{href:string;version:number}>;
    repository: {
        notificationRows(companyId: string, userId: string): Promise<{
            results: Omit<StaffNotification, "href">[];
        }>;
        activityLead(record_id: string, companyId: string): Promise<{
            lead_id: string;
        } | null>;
    };
    page(actor: WorkContext, query: {
        view?: string;
        cursor?: string;
    }): Promise<ListPage<Omit<StaffNotification, 'href'> & {
        page_key: string;
    }>>;
    organizationChangeNotificationTarget(...args: Tail<Parameters<typeof import("@erpx/server-organization/public/organization-work").organizationChangeNotificationTarget>>): ReturnType<typeof import("@erpx/server-organization/public/organization-work").organizationChangeNotificationTarget>;
    companyBusinessAcceptanceWork(...args: Tail<Parameters<typeof import("@erpx/server-finance/public/company-business-acceptance").companyBusinessAcceptanceWork>>): ReturnType<typeof import("@erpx/server-finance/public/company-business-acceptance").companyBusinessAcceptanceWork>;
    customerIdentityDetail(...args: Parameters<typeof import("@erpx/server-customer_authority/public/customer-identity-changes").customerIdentityDetail>): ReturnType<typeof import("@erpx/server-customer_authority/public/customer-identity-changes").customerIdentityDetail>;
    customerDuplicateDetail(...args: Parameters<typeof import("@erpx/server-customer_authority/public/customer-duplicates").customerDuplicateDetail>): ReturnType<typeof import("@erpx/server-customer_authority/public/customer-duplicates").customerDuplicateDetail>;
    customerAccessBinding(...args: Parameters<typeof import("@erpx/server-customer_authority/public/customer-access").customerAccessBinding>): ReturnType<typeof import("@erpx/server-customer_authority/public/customer-access").customerAccessBinding>;
    getSupportAlert(...args: Parameters<typeof import("@erpx/server-support/public/support-sla-alerts").getSupportAlert>): ReturnType<typeof import("@erpx/server-support/public/support-sla-alerts").getSupportAlert>;
    getSupportIncident(...args: Parameters<typeof import("@erpx/server-support/public/support-incidents").getSupportIncident>): ReturnType<typeof import("@erpx/server-support/public/support-incidents").getSupportIncident>;
    getSupportPolicy(...args: Parameters<typeof import("@erpx/server-support/public/support-sla").getSupportPolicy>): ReturnType<typeof import("@erpx/server-support/public/support-sla").getSupportPolicy>;
    getSupportTicket(...args: Parameters<typeof import("@erpx/server-support/public/support").getSupportTicket>): ReturnType<typeof import("@erpx/server-support/public/support").getSupportTicket>;
    operationsExceptionDetail(...args: Parameters<typeof import("@erpx/server-operations/public/operations-exceptions").operationsExceptionDetail>): ReturnType<typeof import("@erpx/server-operations/public/operations-exceptions").operationsExceptionDetail>;
    supportSessionDetail(...args: Parameters<typeof import("@erpx/server-support/public/support-sessions").supportSessionDetail>): ReturnType<typeof import("@erpx/server-support/public/support-sessions").supportSessionDetail>;
    canReadAccessReviewAlert(...args: Tail<Parameters<typeof import("@erpx/server-identity_access/public/iam-access-review-alerts").canReadAccessReviewAlert>>): ReturnType<typeof import("@erpx/server-identity_access/public/iam-access-review-alerts").canReadAccessReviewAlert>;
    payrollCorrectionAlertTarget(...args: Tail<Parameters<typeof import("@erpx/server-people_payroll/public/people-payroll").payrollCorrectionAlertTarget>>): ReturnType<typeof import("@erpx/server-people_payroll/public/people-payroll").payrollCorrectionAlertTarget>;
    payrollAdvanceAlertTarget(...args: Tail<Parameters<typeof import("@erpx/server-people_payroll/public/people-payroll").payrollAdvanceAlertTarget>>): ReturnType<typeof import("@erpx/server-people_payroll/public/people-payroll").payrollAdvanceAlertTarget>;
    getTask(...args: Tail<Parameters<typeof import("@erpx/server-operations/public/staff-tasks").getTask>>): ReturnType<typeof import("@erpx/server-operations/public/staff-tasks").getTask>;
    maintenanceDetail(...args: Parameters<typeof import("@erpx/server-maintenance/public/asset-maintenance").maintenanceDetail>): ReturnType<typeof import("@erpx/server-maintenance/public/asset-maintenance").maintenanceDetail>;
    preventivePlanDetail(...args: Parameters<typeof import("@erpx/server-maintenance/public/maintenance-preventive").preventivePlanDetail>): ReturnType<typeof import("@erpx/server-maintenance/public/maintenance-preventive").preventivePlanDetail>;
    projectHandoffTarget(...args: Tail<Parameters<typeof import("@erpx/server-projects/public/project-structure").projectHandoffTarget>>): ReturnType<typeof import("@erpx/server-projects/public/project-structure").projectHandoffTarget>;
    fieldServiceDetail(...args: Parameters<typeof import("@erpx/server-field_service/public/field-service").fieldServiceDetail>): ReturnType<typeof import("@erpx/server-field_service/public/field-service").fieldServiceDetail>;
    getOrder(...args: Parameters<typeof import("@erpx/server-commerce/public/commerce").getOrder>): ReturnType<typeof import("@erpx/server-commerce/public/commerce").getOrder>;
    collaborationActionDetail(...args: Parameters<typeof import("@erpx/server-collaboration/public/collaboration-work").collaborationActionDetail>): ReturnType<typeof import("@erpx/server-collaboration/public/collaboration-work").collaborationActionDetail>;
    learningAssignmentDetail(...args: Parameters<typeof import("@erpx/server-learning/public/learning").learningAssignmentDetail>): ReturnType<typeof import("@erpx/server-learning/public/learning").learningAssignmentDetail>;
    benefitDetail(...args: Parameters<typeof import("@erpx/server-benefits/public/benefits").benefitDetail>): ReturnType<typeof import("@erpx/server-benefits/public/benefits").benefitDetail>;
    talentDetail(...args: Parameters<typeof import("@erpx/server-talent/public/talent").talentDetail>): ReturnType<typeof import("@erpx/server-talent/public/talent").talentDetail>;
    marketingDetail(...args: Parameters<typeof import("@erpx/server-marketing/public/marketing").marketingDetail>): ReturnType<typeof import("@erpx/server-marketing/public/marketing").marketingDetail>;
    productionDetail(...args: Parameters<typeof import("@erpx/server-manufacturing/public/production").productionDetail>): ReturnType<typeof import("@erpx/server-manufacturing/public/production").productionDetail>;
    getCRMLead(...args: Parameters<typeof import("@erpx/server-crm/public/crm").getCRMLead>): ReturnType<typeof import("@erpx/server-crm/public/crm").getCRMLead>;
    getLandedCost(...args: Parameters<typeof import("@erpx/server-inventory/public/inventory-landed-costs").getLandedCost>): ReturnType<typeof import("@erpx/server-inventory/public/inventory-landed-costs").getLandedCost>;
    getCount(...args: Parameters<typeof import("@erpx/server-inventory/public/inventory-counts").getCount>): ReturnType<typeof import("@erpx/server-inventory/public/inventory-counts").getCount>;
    getCustomerFollowup(...args: Parameters<typeof import("@erpx/server-sales/public/customer-followups").getCustomerFollowup>): ReturnType<typeof import("@erpx/server-sales/public/customer-followups").getCustomerFollowup>;
    getSupplierBill(...args: Parameters<typeof import("@erpx/server-procurement/public/supplier-bill-matching").getSupplierBill>): ReturnType<typeof import("@erpx/server-procurement/public/supplier-bill-matching").getSupplierBill>;
    getRfq(...args: Parameters<typeof import("@erpx/server-procurement/public/procurement-rfqs").getRfq>): ReturnType<typeof import("@erpx/server-procurement/public/procurement-rfqs").getRfq>;
    getSupplierPromiseOrder(...args: Parameters<typeof import("@erpx/server-procurement/public/supplier-promises").getSupplierPromiseOrder>): ReturnType<typeof import("@erpx/server-procurement/public/supplier-promises").getSupplierPromiseOrder>;
    getTransit(...args: Parameters<typeof import("@erpx/server-inventory/public/inventory-transit").getTransit>): ReturnType<typeof import("@erpx/server-inventory/public/inventory-transit").getTransit>;
    getQualityHold(...args: Parameters<typeof import("@erpx/server-quality/public/quality").getQualityHold>): ReturnType<typeof import("@erpx/server-quality/public/quality").getQualityHold>;
    getQualityAlert(...args: Parameters<typeof import("@erpx/server-quality/public/quality-alerts").getQualityAlert>): ReturnType<typeof import("@erpx/server-quality/public/quality-alerts").getQualityAlert>;
    getSupplierBatch(...args: Parameters<typeof import("@erpx/server-procurement/public/supplier-payments").getSupplierBatch>): ReturnType<typeof import("@erpx/server-procurement/public/supplier-payments").getSupplierBatch>;
    getRefund(...args: Parameters<typeof import("@erpx/server-finance/public/refunds").getRefund>): ReturnType<typeof import("@erpx/server-finance/public/refunds").getRefund>;
    access(...args: Tail<Parameters<typeof import("@erpx/server-legal/public/legal/access").access>>): ReturnType<typeof import("@erpx/server-legal/public/legal/access").access>;
    retirementWork(...args: Tail<Parameters<typeof import("@erpx/server-people_payroll/public/retirement-clearance").retirementWork>>): ReturnType<typeof import("@erpx/server-people_payroll/public/retirement-clearance").retirementWork>;
    rosterNotificationLink(...args: Tail<Parameters<typeof import("@erpx/server-people_payroll/public/people-roster").rosterNotificationLink>>): ReturnType<typeof import("@erpx/server-people_payroll/public/people-roster").rosterNotificationLink>;
    rosterData(...args: Tail<Parameters<typeof import("@erpx/server-people_payroll/public/people-roster").rosterData>>): ReturnType<typeof import("@erpx/server-people_payroll/public/people-roster").rosterData>;
    getOvertime(...args: Parameters<typeof import("@erpx/server-people_payroll/public/overtime").getOvertime>): ReturnType<typeof import("@erpx/server-people_payroll/public/overtime").getOvertime>;
    getLeave(...args: Parameters<typeof import("@erpx/server-people_payroll/public/leave").getLeave>): ReturnType<typeof import("@erpx/server-people_payroll/public/leave").getLeave>;
    getExpense(...args: Parameters<typeof import("@erpx/server-people_payroll/public/expenses").getExpense>): ReturnType<typeof import("@erpx/server-people_payroll/public/expenses").getExpense>;
    getEmployeeRequest(...args: Tail<Parameters<typeof import("@erpx/server-records/public/employee-requests").getEmployeeRequest>>): ReturnType<typeof import("@erpx/server-records/public/employee-requests").getEmployeeRequest>;
    readablePost(...args: Tail<Parameters<typeof import("@erpx/server-collaboration/public/social-source").readablePost>>): ReturnType<typeof import("@erpx/server-collaboration/public/social-source").readablePost>;
}
export function createStaffNotificationApplication(ports: StaffNotificationPorts) {
    const r = ports.repository;
    function hasStaffNotificationResolver(type: string) { return (staffNotificationSourceTypes as readonly string[]).includes(type); }
    async function listStaffNotifications(actor: WorkContext) {
        const rows = (await r.notificationRows(actor.companyId, actor.userId)).results;
        return visibleNotifications(actor, rows);
    }
    async function visibleNotifications(actor: WorkContext, rows: Omit<StaffNotification, "href">[]) {
        // A denied source disappears from this projection; it must not interrupt
        // the whole Inbox page. Owner readers still enforce the current authority.
        actor = { ...actor, pageRequest: undefined };
        let retirement: ReturnType<StaffNotificationPorts["retirementWork"]> | undefined;
        const visible: StaffNotification[] = [];
        for (const row of rows) {
            try {
                if(row.record_type==='handbook'||row.record_type==='handbook_article'){
                    const target=await ports.handbookNotificationTarget(actor,row.record_id);visible.push({...row,href:target.href,source:{kind:'handbook_article',id:row.record_id,version:target.version,href:target.href,label:sourceRecordLabel('handbook_article')}});continue;
                }
                if(row.record_type==="invoice"||row.record_type==="journal"){
                    const target=await ports.financeNotificationTarget(actor,row.record_type,row.record_id);
                    visible.push({...row,href:target.href,source:{kind:row.record_type,id:row.record_id,version:target.version,href:target.href,label:sourceRecordLabel(row.record_type)}});continue;
                }
                if (row.record_type === "organization_change") {
                    const href = await ports.organizationChangeNotificationTarget(actor, row.record_id);
                    if (href)
                        visible.push({ ...row, href });
                    continue;
                }
                if (row.record_type === "company_business_acceptance") {
                    const item = (await ports.companyBusinessAcceptanceWork(actor)).find(work => work.id === row.record_id);
                    if (item)
                        visible.push({ ...row, href: item.href });
                    continue;
                }
                if (row.record_type === "customer_identity") {
                    await ports.customerIdentityDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/customer-identity/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "customer_duplicate") {
                    await ports.customerDuplicateDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/customer-duplicates/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "customer_access") {
                    await ports.customerAccessBinding(actor, row.record_id);
                    visible.push({ ...row, href: `/customer-access/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "support_sla_alert") {
                    await ports.getSupportAlert(actor, row.record_id);
                    visible.push({ ...row, href: "/support/sla" });
                    continue;
                }
                if (row.record_type === "support_incident") {
                    await ports.getSupportIncident(actor, row.record_id);
                    visible.push({ ...row, href: `/support/incidents/${row.record_id}` });
                    continue;
                }
                if (row.record_type === "support_policy") {
                    await ports.getSupportPolicy(actor, row.record_id);
                    visible.push({ ...row, href: "/support/sla" });
                    continue;
                }
                if (row.record_type === "support_ticket") {
                    await ports.getSupportTicket(actor, row.record_id);
                    visible.push({ ...row, href: `/support/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "operational_exception") {
                    await ports.operationsExceptionDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/operations/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "support_session") {
                    await ports.supportSessionDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/support/sessions/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "iam_access_review") {
                    if (await ports.canReadAccessReviewAlert(actor.companyId, actor.userId, row.record_id))
                        visible.push({ ...row, href: `/people/settings?section=reviews#${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "payroll_correction") {
                    const runId = await ports.payrollCorrectionAlertTarget(actor.companyId, actor.userId, row.record_id);
                    if (runId)
                        visible.push({ ...row, href: `/people/payroll/runs/${encodeURIComponent(runId)}#correction-${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "payroll_advance") {
                    const advanceId = await ports.payrollAdvanceAlertTarget(actor.companyId, actor.userId, row.record_id);
                    if (advanceId)
                        visible.push({ ...row, href: `/people/payroll/advances#payroll-advance-${encodeURIComponent(advanceId)}` });
                    continue;
                }
                if (row.record_type === "staff_task_alert") {
                    await ports.getTask(actor, row.record_id);
                    visible.push({ ...row, href: `/staff-tasks/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "maintenance_work") {
                    await ports.maintenanceDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/maintenance/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "maintenance_plan") {
                    await ports.preventivePlanDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/maintenance/plans/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "project_handoff") {
                    visible.push({ ...row, href: await ports.projectHandoffTarget(actor, row.record_id) });
                    continue;
                }
                if (row.record_type === "field_service_order") {
                    await ports.fieldServiceDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/field-service/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "commerce_order") {
                    await ports.getOrder(actor, row.record_id);
                    visible.push({ ...row, href: `/commerce/orders/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "collaboration_action") {
                    await ports.collaborationActionDetail(actor, row.record_id);
                    visible.push({ ...row, href: "/collaboration" });
                    continue;
                }
                if (row.record_type === "learning_assignment") {
                    await ports.learningAssignmentDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/learning/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "benefit_election") {
                    await ports.benefitDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/benefits/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "talent_application") {
                    await ports.talentDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/talent/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "marketing_candidate") {
                    await ports.marketingDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/marketing/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "manufacturing_order") {
                    await ports.productionDetail(actor, row.record_id);
                    visible.push({ ...row, href: `/manufacturing/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "crm_lead") {
                    await ports.getCRMLead(actor, row.record_id);
                    visible.push({ ...row, href: `/crm/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "crm_activity") {
                    const record = await r.activityLead(row.record_id, actor.companyId);
                    if (record) {
                        await ports.getCRMLead(actor, record.lead_id);
                        visible.push({ ...row, href: `/crm/${encodeURIComponent(record.lead_id)}#crm-activities` });
                    }
                    continue;
                }
                if (row.record_type === "landed_cost") {
                    const record = await ports.getLandedCost(actor, row.record_id);
                    visible.push({ ...row, href: `/commerce/orders/${encodeURIComponent(String(record.order_id))}#landed-costs` });
                    continue;
                }
                if (row.record_type === "stock_count") {
                    await ports.getCount(actor, row.record_id);
                    visible.push({ ...row, href: `/stock-counts/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "customer_followup") {
                    await ports.getCustomerFollowup(actor, row.record_id);
                    visible.push({ ...row, href: `/customer-followups/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "supplier_bill") {
                    await ports.getSupplierBill(actor, row.record_id);
                    visible.push({ ...row, href: `/supplier-bills/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "procurement_rfq") {
                    await ports.getRfq(actor, row.record_id);
                    visible.push({ ...row, href: `/procurement-rfqs/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "supplier_promise") {
                    await ports.getSupplierPromiseOrder(actor, row.record_id);
                    visible.push({ ...row, href: `/commerce/orders/${encodeURIComponent(row.record_id)}#supplier-promises` });
                    continue;
                }
                if (row.record_type === "inventory_transit") {
                    await ports.getTransit(actor, row.record_id);
                    visible.push({ ...row, href: `/stock-transit/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "quality_hold") {
                    await ports.getQualityHold(actor, row.record_id);
                    visible.push({ ...row, href: `/quality/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "quality_alert") {
                    const alert = await ports.getQualityAlert(actor, row.record_id);
                    const href = `/quality/${encodeURIComponent(alert.hold_id)}`;
                    visible.push({ ...row, href, source: { kind: "quality_hold", id: alert.hold_id, href, label: sourceRecordLabel("quality_hold") } });
                    continue;
                }
                if (row.record_type === "supplier_batch") {
                    await ports.getSupplierBatch(actor, row.record_id);
                    visible.push({ ...row, href: `/finance/supplier-batches/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "refund") {
                    await ports.getRefund(actor, row.record_id);
                    visible.push({ ...row, href: `/finance/refunds/${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "legal") {
                    await ports.access(actor, row.record_id);
                    visible.push({ ...row, href: `/legal?record=${encodeURIComponent(row.record_id)}` });
                    continue;
                }
                if (row.record_type === "retirement_clearance") {
                    const task = (await (retirement ??= ports.retirementWork(actor))).find(t => t.id === row.record_id);
                    if (task)
                        visible.push({ ...row, href: task.href });
                    continue;
                }
                if (["roster_week", "roster_request"].includes(row.record_type)) {
                    visible.push({ ...row, href: await ports.rosterNotificationLink(actor, row.record_type, row.record_id) });
                    continue;
                }
                if (row.record_type === "roster_coverage") {
                    const match = row.record_id.match(/^(.*):(\d{4}-\d{2}-\d{2}):([^:]+)$/);
                    if (match) {
                        const data = await ports.rosterData(actor, { department: match[1], week: match[2] });
                        if (data.canManage && data.coverageGaps.some(g => g.week_start === match[2] && g.person_id === match[3]))
                            visible.push({ ...row, href: `/people/roster?department=${encodeURIComponent(match[1])}&week=${match[2]}` });
                    }
                    continue;
                }
                if (row.record_type === "overtime") {
                    await ports.getOvertime(actor, row.record_id);
                    visible.push({ ...row, href: `/overtime/${row.record_id}` });
                    continue;
                }
                let sourceKind = row.record_type, sourceVersion: number | undefined;
                if (["leave", "leave_cancellation"].includes(row.record_type)) {
                    const record = await ports.getLeave(actor.holdingId, row.record_id, actor);
                    sourceKind = "leave";
                    sourceVersion = record.version;
                }
                else if (row.record_type === "expense") {
                    const record = await ports.getExpense(actor, row.record_id);
                    sourceVersion = Number(record.version);
                }
                else if (row.record_type === "request") {
                    const record = await ports.getEmployeeRequest(actor, row.record_id);
                    sourceKind = `${record.kind}_request`;
                    sourceVersion = record.version;
                }
                else if (row.record_type === "task") {
                    const record = await ports.getTask(actor, row.record_id);
                    sourceKind = "staff_task";
                    sourceVersion = record.version;
                }
                else if (row.record_type === "post") {
                    const record = await ports.readablePost(actor, row.record_id);
                    sourceVersion = record.version;
                }
                else
                    continue;
                const href = row.record_type === "request" ? `/requests/${row.record_id}` : row.record_type === "post" ? `/posts/${row.record_id}` :
                    ["leave", "leave_cancellation"].includes(row.record_type)
                        ? `/tasks/people_payroll/${row.record_id}`
                        : row.record_type === "expense"
                            ? `/expenses/${row.record_id}`
                            : `/staff-tasks/${row.record_id}`;
                visible.push({ ...row, href, source: { kind: sourceKind, id: row.record_id, version: sourceVersion, href, label: sourceRecordLabel(sourceKind) } });
            }
            catch (error) {
                if (!(error instanceof BusinessRuleError) || ![403, 404].includes(error.status))
                    throw error;
            }
        }
        return visible.map(row => ({ ...row, source: row.source ?? { kind: row.record_type === "task" ? "staff_task" : row.record_type, id: row.record_id, href: row.href,
                label: sourceRecordLabel(row.record_type === "task" ? "staff_task" : row.record_type) } }));
    }
    async function notificationsPage(actor: WorkContext, query: {
        view?: string;
        cursor?: string;
    }) {
        const page = await ports.page(actor, query);
        return { ...page, items: await visibleNotifications(actor, page.items) };
    }
    return { hasStaffNotificationResolver, listStaffNotifications, visibleNotifications, notificationsPage };
}
