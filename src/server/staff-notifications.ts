import { readListPage } from "./list-page";
import { getOvertime } from "./overtime";
import { retirementWork } from "./retirement-clearance";
import { rosterNotificationLink } from "./people-roster";
import type { SqlDatabase } from "./runtime/types";
import { getEmployeeRequest } from "./employee-requests";
import { readablePost } from "./social-source";
import type { WorkContext } from "./authorization";
import { getLeave } from "./leave";
import { getExpense } from "./expenses";
import { getTask } from "./staff-tasks";
import { ApiError } from "./http";
export type StaffNotification = {
  id: string;
  record_type: string;
  record_id: string;
  record_version: number;
  message: string;
  created_at: number;
  read_at: number | null;
  href: string;
};
export async function listStaffNotifications(
  db: SqlDatabase,
  actor: WorkContext,
) {
  const rows = (
    await db
      .prepare(
        "SELECT * FROM staff_notifications WHERE company_id=? AND user_id=? ORDER BY created_at DESC,id LIMIT 100",
      )
      .bind(actor.companyId, actor.userId)
      .all<Omit<StaffNotification, "href">>()
  ).results;
  return visibleNotifications(db,actor,rows);
}
async function visibleNotifications(db:SqlDatabase,actor:WorkContext,rows:Omit<StaffNotification,"href">[]) {
  let retirement: ReturnType<typeof retirementWork> | undefined;
  const visible: StaffNotification[] = [];
  for (const row of rows) {
    try {
      if(row.record_type === "organization_change") {
        const href=await (await import("./organization-work")).organizationChangeNotificationTarget(db,actor,row.record_id);
        if(href)visible.push({...row,href});
        continue;
      }
      if(row.record_type === "company_business_acceptance") {
        const item=(await (await import("./company-business-acceptance")).companyBusinessAcceptanceWork(db,actor)).find(work=>work.id===row.record_id);
        if(item)visible.push({...row,href:item.href});
        continue;
      }
      if(row.record_type === "customer_identity") {await (await import("./customer-identity-changes")).customerIdentityDetail(actor,row.record_id);visible.push({...row,href:`/customer-identity/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "customer_duplicate") {await (await import("./customer-duplicates")).customerDuplicateDetail(actor,row.record_id);visible.push({...row,href:`/customer-duplicates/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "customer_access") {await (await import("./customer-access")).customerAccessBinding(actor,row.record_id);visible.push({...row,href:`/customer-access/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "support_sla_alert") {await (await import("./support-sla-alerts")).getSupportAlert(actor,row.record_id);visible.push({...row,href:"/support/sla"});continue;}
      if(row.record_type === "support_incident") {await (await import("./support-incidents")).getSupportIncident(actor,row.record_id);visible.push({...row,href:`/support/incidents/${row.record_id}`});continue;}
      if(row.record_type === "support_policy") {await (await import("./support-sla")).getSupportPolicy(actor,row.record_id);visible.push({...row,href:"/support/sla"});continue;}
      if(row.record_type === "support_ticket") {await (await import("./support")).getSupportTicket(actor,row.record_id);visible.push({...row,href:`/support/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "support_session") {await (await import("./support-sessions")).supportSessionDetail(actor,row.record_id);visible.push({...row,href:`/support/sessions/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "iam_access_review") {
        if(await (await import("./iam-access-review-alerts")).canReadAccessReviewAlert(db,actor.companyId,actor.userId,row.record_id))visible.push({...row,href:`/people/settings?section=reviews#${encodeURIComponent(row.record_id)}`});
        continue;
      }
      if(row.record_type === "payroll_correction") {
        const runId=await (await import("./people-payroll")).payrollCorrectionAlertTarget(db,actor.companyId,actor.userId,row.record_id);
        if(runId)visible.push({...row,href:`/people/payroll/${encodeURIComponent(runId)}#correction-${encodeURIComponent(row.record_id)}`});
        continue;
      }
      if(row.record_type === "payroll_advance") {
        const advanceId=await (await import("./people-payroll")).payrollAdvanceAlertTarget(db,actor.companyId,actor.userId,row.record_id);
        if(advanceId)visible.push({...row,href:`/people?view=payroll#payroll-advances`});
        continue;
      }
      if(row.record_type === "staff_task_alert") {
        await getTask(db, actor, row.record_id);
        visible.push({...row,href:`/staff-tasks/${encodeURIComponent(row.record_id)}`});
        continue;
      }
      if(row.record_type === "maintenance_work") {await (await import("./asset-maintenance")).maintenanceDetail(actor,row.record_id);visible.push({...row,href:`/maintenance/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "field_service_order") {await (await import("./field-service")).fieldServiceDetail(actor,row.record_id);visible.push({...row,href:`/field-service/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "commerce_order") {await (await import("./commerce")).getOrder(actor,row.record_id);visible.push({...row,href:`/commerce/orders/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "collaboration_action") {await (await import("./collaboration-work")).collaborationActionDetail(actor,row.record_id);visible.push({...row,href:"/collaboration"});continue;}
      if(row.record_type === "learning_assignment") {await (await import("./learning")).learningAssignmentDetail(actor,row.record_id);visible.push({...row,href:`/learning/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "benefit_election") {await (await import("./benefits")).benefitDetail(actor,row.record_id);visible.push({...row,href:`/benefits/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "talent_application") {await (await import("./talent")).talentDetail(actor,row.record_id);visible.push({...row,href:`/talent/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "marketing_candidate") {await (await import("./marketing")).marketingDetail(actor,row.record_id);visible.push({...row,href:`/marketing/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "manufacturing_order") {await (await import("./production")).productionDetail(actor,row.record_id);visible.push({...row,href:`/manufacturing/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "crm_lead") {await (await import("./crm")).getCRMLead(actor,row.record_id);visible.push({...row,href:`/crm/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "customer_followup") {await (await import("./customer-followups")).getCustomerFollowup(actor,row.record_id);visible.push({...row,href:`/customer-followups/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "supplier_bill") {await (await import("./supplier-bill-matching")).getSupplierBill(actor,row.record_id);visible.push({...row,href:`/supplier-bills/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "procurement_rfq") {await (await import("./procurement-rfqs")).getRfq(actor,row.record_id);visible.push({...row,href:`/procurement-rfqs/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "supplier_promise") {await (await import("./supplier-promises")).getSupplierPromiseOrder(actor,row.record_id);visible.push({...row,href:`/commerce/orders/${encodeURIComponent(row.record_id)}#supplier-promises`});continue;}
      if(row.record_type === "inventory_transit") {await (await import("./inventory-transit")).getTransit(actor,row.record_id);visible.push({...row,href:`/stock-transit/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "quality_hold") {await (await import("./quality")).getQualityHold(actor,row.record_id);visible.push({...row,href:`/quality/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "quality_alert") {const alert=await (await import("./quality-alerts")).getQualityAlert(actor,row.record_id);visible.push({...row,href:`/quality/${encodeURIComponent(alert.hold_id)}`});continue;}
      if(row.record_type === "supplier_batch") {await (await import("./supplier-payments")).getSupplierBatch(actor,row.record_id);visible.push({...row,href:`/finance/supplier-batches/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "refund") {await (await import("./refunds")).getRefund(actor,row.record_id);visible.push({...row,href:`/finance/refunds/${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "legal") {await (await import("./legal/access")).access(db,actor,row.record_id);visible.push({...row,href:`/legal?record=${encodeURIComponent(row.record_id)}`});continue;}
      if(row.record_type === "retirement_clearance") {
        const task = (await (retirement ??= retirementWork(db,actor))).find(t=>t.id===row.record_id);
        if(task)visible.push({...row,href:task.href});
        continue;
      }
      if (["roster_week", "roster_request"].includes(row.record_type)) {
        visible.push({ ...row, href: await rosterNotificationLink(db, actor, row.record_type, row.record_id) });
        continue;
      }
      if(row.record_type === "overtime") {await getOvertime(actor,row.record_id);visible.push({...row,href:`/overtime/${row.record_id}`});continue;}
      if (["leave","leave_cancellation"].includes(row.record_type))
        await getLeave(actor.holdingId, row.record_id, actor);
      else if (row.record_type === "expense")
        await getExpense(actor, row.record_id);
      else if (row.record_type === "request") await getEmployeeRequest(db,actor,row.record_id);
      else if (row.record_type === "task")
        await getTask(db, actor, row.record_id);
      else if (row.record_type === "post") await readablePost(db,actor,row.record_id);
      else continue;
      visible.push({
        ...row,
        href: row.record_type === "request" ? `/requests/${row.record_id}` : row.record_type === "post" ? `/posts/${row.record_id}` :
          ["leave","leave_cancellation"].includes(row.record_type)
            ? `/tasks/people_payroll/${row.record_id}`
            : row.record_type === "expense"
              ? `/expenses/${row.record_id}`
              : `/staff-tasks/${row.record_id}`,
      });
    } catch (error) {
      if (!(error instanceof ApiError) || ![403, 404].includes(error.status))
        throw error;
    }
  }
  return visible;
}

export async function notificationsPage(db:SqlDatabase,actor:WorkContext,query:{view?:string;cursor?:string}) {
  const page=await readListPage<Omit<StaffNotification,"href"> & {page_key:string}>(db,{
    sql:`SELECT *,created_at::text AS page_key FROM staff_notifications WHERE company_id=? AND user_id=? ${query.view==='all'?'':'AND read_at IS NULL'}`,
    args:[actor.companyId,actor.userId],descending:true,cursor:query.cursor,scope:[actor.companyId,actor.userId,'notifications',query.view??'unread'],
  });
  return {...page,items:await visibleNotifications(db,actor,page.items)};
}
