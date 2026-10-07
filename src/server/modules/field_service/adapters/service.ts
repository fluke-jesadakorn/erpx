import { finalRootOwnerProjectionSources as assetsProjectionSources } from "../../assets/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as financeProjectionSources } from "../../finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as inventoryProjectionSources } from "../../inventory/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as supportProjectionSources } from "../../support/public/final-root-owner-projection-sources";
import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {holdingResources} from "../../runtime/public/runtime";
import {requirePermission} from "../../identity_access/public/iam-permissions";
import {hasPermission} from "@/lib/permissions";
import {hasModuleAccess} from "@/data/module-access";
import {assertion,executeCommand,replayCommand} from "../../workflow/public/commands";
import {ApiError} from "../../runtime/public/http";
import {availableServiceSpares,prepareInventoryConsumption} from "../../inventory/public/inventory-consumption";
import {prepareFieldServiceStockReservation} from "../../inventory/public/field-service-reservations";
import {prepareFieldServiceInvoice} from "../../finance/public/field-service-billing";
import {prepareFieldServiceNotification} from "../../collaboration/public/field-service-notifications";
import {BusinessRuleError} from "../../shared";
import {createFieldServiceApplication,type FieldServicePorts,type FieldServiceMutation} from "../application/service";
import type {Agreement,Order,OrderFacts,FieldRow as Row} from "../domain/service";
export {createAgreement} from "./agreement";
type Consumption=Awaited<ReturnType<typeof prepareInventoryConsumption>>;
type Billing=Awaited<ReturnType<typeof prepareFieldServiceInvoice>>;
type Spares=Awaited<ReturnType<typeof availableServiceSpares>>;
const aScope =
    "g.company_id=? AND (?::text IS NULL OR g.branch_id=?) AND (?::text IS NULL OR g.department_id=?)",
  oScope =
    "o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)",
  args = (a: WorkContext) => [
    a.companyId,
    a.branchId,
    a.branchId,
    a.departmentId,
    a.departmentId,
  ];
const technician = `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) x JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=o.company_id AND x.user_id=o.technician_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=o.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=o.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')`;
const reviewer = (alias: "g" | "o") =>
  `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) x JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=${alias}.company_id AND x.user_id=? AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.review') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=${alias}.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=${alias}.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')`;
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "field_service"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Field Service.",
    );
}
async function readWorkspace(write:SqlDatabase,a:WorkContext){
 const agreements = (
      await write
        .prepare(
          `SELECT g.*,p.name customer_name,a.code asset_code,CASE WHEN ${reviewer("g")} THEN 1 ELSE 0 END can_review FROM field_service_agreements g JOIN (${financeProjectionSources.finance_parties}) p ON p.id=g.customer_id LEFT JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=g.asset_id WHERE ${aScope} ORDER BY g.updated_at DESC,g.id`,
        )
        .bind(a.userId, ...args(a))
        .all<Agreement>()
    ).results,
    orders = (
      await write
        .prepare(
          `SELECT o.*,t.title ticket_title,p.name customer_name,g.title agreement_title,a.code asset_code,CASE WHEN ${technician} THEN 1 ELSE 0 END technician_eligible,CASE WHEN ${reviewer("o")} THEN 1 ELSE 0 END can_review,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=o.company_id AND user_id=o.technician_id AND merged_into_id IS NULL LIMIT 1) technician_name FROM field_service_orders o JOIN (${supportProjectionSources.support_tickets}) t ON t.id=o.ticket_id JOIN (${financeProjectionSources.finance_parties}) p ON p.id=t.customer_id JOIN field_service_agreements g ON g.id=o.agreement_id LEFT JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=o.asset_id WHERE ${oScope} ORDER BY o.updated_at DESC,o.id`,
        )
        .bind(a.userId, ...args(a))
        .all<Order>()
    ).results,
    customers = (
      await write
        .prepare(
          `SELECT id,name FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name`,
        )
        .bind(a.companyId)
        .all<Row>()
    ).results,
    assets = (
      await write
        .prepare(
          `SELECT id,code,name,version FROM (${assetsProjectionSources.operational_assets}) operational_assets WHERE company_id=? AND lifecycle_status<>'retired' ORDER BY code`,
        )
        .bind(a.companyId)
        .all<Row>()
    ).results,
    tickets = (
      await write
        .prepare(
          `SELECT id,title,version,customer_id,branch_id,department_id FROM (${supportProjectionSources.support_tickets}) support_tickets WHERE company_id=? AND source_kind='sales_order' AND status IN ('open','in_progress','waiting_customer') ORDER BY updated_at DESC`,
        )
        .bind(a.companyId)
        .all<Row>()
    ).results,
    people = (
      await write
        .prepare(
          `SELECT DISTINCT p.user_id id,p.legal_name name FROM (${people_payrollProjectionSources.hr_people}) p JOIN (${identity_accessProjectionSources.assignments}) x ON x.user_id=p.user_id AND x.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') ORDER BY p.legal_name`,
        )
        .bind(a.companyId)
        .all<Row>()
    ).results,
    nodes = (
      await write
        .prepare(
          `SELECT id,kind,display_name,branch_id FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE company_id=? AND status='active' AND kind IN ('branch','headquarters','department') ORDER BY kind,display_name`,
        )
        .bind(a.companyId)
        .all<Row>()
    ).results;
  return {
    agreements,
    orders,
    customers,
    assets,
    tickets,
    people,
    nodes,
    canWrite: hasPermission(a, "records.write"),
  };
}
async function readDetail(write:SqlDatabase,a:WorkContext,order:Order){
 const id=order.id;
  return {
    spares:await availableServiceSpares(write,a,String(order.branch_id)),
    parts: (
      await write
        .prepare(
          `SELECT r.*,p.sku,p.name FROM field_service_part_reservations r JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=r.product_id WHERE r.order_id=? ORDER BY r.created_at,r.id`,
        )
        .bind(id)
        .all<Row>()
    ).results,
    consumptions:(await write.prepare(`SELECT p.*,j.status journal_status,product.name product_name FROM (${inventoryProjectionSources.operational_inventory_consumptions}) p JOIN (${financeProjectionSources.finance_journals}) j ON j.id=p.journal_id JOIN (${commerceProjectionSources.commerce_products}) product ON product.id=p.product_id WHERE p.source_kind='field_service' AND p.source_id=? ORDER BY p.created_at,p.id`).bind(id).all<Row>()).results,
    history: (
      await write
        .prepare(
          "SELECT * FROM field_service_order_events WHERE order_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>()
    ).results,
    billing: await write
      .prepare("SELECT * FROM field_service_billing_releases WHERE order_id=?")
      .bind(id)
      .first<Row>(),
  };
}
async function commitAgreement<Result>(write:SqlDatabase,a:WorkContext,m:Extract<FieldServiceMutation<Consumption,Billing>,{action:"agreement"}>,result:Result):Promise<Result>{
 const {key,id,input,operation:op,now}=m,{expected,status}=m.transition;
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    input.expectedVersion + 1,
    result,
    [
      assertion(
        write,
        key,
        "review",
        `EXISTS(SELECT 1 FROM field_service_agreements g WHERE g.id=? AND g.version=? AND g.status=? AND g.created_by<>? AND ${reviewer("g")})`,
        [id, input.expectedVersion, expected, a.userId, a.userId],
      ),
      ...(input.action === "retire"
        ? [
            assertion(
              write,
              key,
              "no-open-orders",
              "NOT EXISTS(SELECT 1 FROM field_service_orders WHERE agreement_id=? AND status NOT IN ('completed','cancelled','billed'))",
              [id],
            ),
          ]
        : []),
      write
        .prepare(
          "UPDATE field_service_agreements SET status=?,reviewed_by=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(status, a.userId, now, now, id),
      write
        .prepare(
          "INSERT INTO field_service_agreement_events VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          `service-agreement-event-${key}`,
          id,
          input.expectedVersion + 1,
          input.action,
          a.userId,
          input.reason,
          status,
          now,
        ),
    ],
  );
}
async function commitCreateOrder<Result>(write:SqlDatabase,a:WorkContext,m:Extract<FieldServiceMutation<Consumption,Billing>,{action:"create-order"}>,result:Result):Promise<Result>{
 const {key,id,input,operation:op,now,status}=m;
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    1,
    result,
    [
      assertion(
        write,
        key,
        "source",
        `EXISTS(SELECT 1 FROM (${supportProjectionSources.support_tickets}) t JOIN field_service_agreements g ON g.id=? AND g.customer_id=t.customer_id AND g.company_id=t.company_id AND g.branch_id=t.branch_id AND g.department_id IS NOT DISTINCT FROM t.department_id WHERE t.id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer') AND g.version=? AND g.status='active' AND g.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND g.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?))`,
        [
          input.agreementId,
          input.ticketId,
          input.ticketVersion,
          input.agreementVersion,
          ...args(a),
        ],
      ),
      ...(input.assetId
        ? [
            assertion(
              write,
              key,
              "asset",
              `EXISTS(SELECT 1 FROM (${assetsProjectionSources.operational_assets}) a JOIN field_service_agreements g ON g.id=? WHERE a.id=? AND a.company_id=g.company_id AND a.branch_id=g.branch_id AND (g.asset_id IS NULL OR g.asset_id=a.id) AND a.lifecycle_status NOT IN ('retired','retirement_pending'))`,
              [input.agreementId, input.assetId],
            ),
          ]
        : []),
      write
        .prepare(
          `INSERT INTO field_service_orders(id,company_id,branch_id,department_id,ticket_id,ticket_version,agreement_id,agreement_version,asset_id,issue,site,technician_id,scheduled_at,status,created_by,created_at,updated_at) SELECT ?,company_id,branch_id,department_id,id,?,?,?,?,?,?,?,?,?,?,?,? FROM (${supportProjectionSources.support_tickets}) support_tickets WHERE id=?`,
        )
        .bind(
          id,
          input.ticketVersion,
          input.agreementId,
          input.agreementVersion,
          input.assetId ?? null,
          input.issue,
          input.site,
          input.technicianId,
          input.scheduledAt ?? null,
          status,
          a.userId,
          now,
          now,
          input.ticketId,
        ),
      assertion(
        write,
        key,
        "technician",
        `EXISTS(SELECT 1 FROM field_service_orders o WHERE o.id=? AND ${technician})`,
        [id],
      ),
      write
        .prepare(
          "INSERT INTO field_service_order_events VALUES(?,?,1,'create',?,?,?,?,?,?::jsonb,?)",
        )
        .bind(
          `service-order-event-${key}`,
          id,
          a.userId,
          input.issue,
          status,
          input.technicianId,
          input.scheduledAt ?? null,
          JSON.stringify({ site: input.site }),
          now,
        ),
      prepareFieldServiceNotification(write,{key,companyId:a.companyId,technicianId:input.technicianId,orderId:id,version:1,created:true,now}),
    ],
  );
}
async function commitOrder<Result>(write:SqlDatabase,a:WorkContext,m:Extract<FieldServiceMutation<Consumption,Billing>,{action:"order"}>,result:Result):Promise<Result>{
 const {key,id,input,operation:op,now,order:row}=m,{status,technicianId:tech,scheduledAt:scheduled,exceptionReason,details}=m.transition;
 const steps = [
      assertion(
        write,
        key,
        "version",
        `EXISTS(SELECT 1 FROM field_service_orders o WHERE o.id=? AND o.version=? AND ${oScope})`,
        [id, input.expectedVersion, ...args(a)],
      ),
    ];
  if (input.action === "schedule") {
    steps.push(
      assertion(
        write,
        key,
        "technician",
        `EXISTS(SELECT 1 FROM field_service_orders o WHERE o.id=? AND ${technician.replaceAll("o.technician_id", "?::text")})`,
        [id, input.technicianId],
      ),
    );
  } else if (input.action === "reserve-part") {
    steps.push(
      ...prepareFieldServiceStockReservation(write,a,{key,branchId:String(row.branch_id),warehouseId:input.warehouseId,productId:input.productId,quantityMilli:input.quantityMilli}),
      write
        .prepare(
          "INSERT INTO field_service_part_reservations(id,order_id,warehouse_id,product_id,quantity_milli,expires_at,created_by,reason,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          `field-part-${key}`,
          id,
          input.warehouseId,
          input.productId,
          input.quantityMilli,
          input.expiresAt,
          a.userId,
          input.reason,
          now,
        ),
    );
  } else if (input.action === "dispatch") {
    steps.push(
      assertion(
        write,
        key,
        "parts-current",
        "NOT EXISTS(SELECT 1 FROM field_service_part_reservations WHERE order_id=? AND quantity_milli>consumed_milli+released_milli AND expires_at<=?)",
        [id, now],
      ),
    );
  } else if (input.action === "start") {
    steps.push(
      write
        .prepare(
          "UPDATE field_service_orders SET visit_started_at=? WHERE id=?",
        )
        .bind(now, id),
    );
  } else if(input.action==='issue-part'){
    steps.push(...m.consumption!.steps);
  } else if (input.action === "complete") {
    steps.push(assertion(write,key,'parts-gl-posted',`NOT EXISTS(SELECT 1 FROM (${inventoryProjectionSources.operational_inventory_consumptions}) p JOIN (${financeProjectionSources.finance_journals}) j ON j.id=p.journal_id WHERE p.source_kind='field_service' AND p.source_id=? AND j.status<>'posted')`,[id]));
    steps.push(
      write
        .prepare(
          "UPDATE field_service_orders SET visit_completed_at=?,labor_minutes=?,completion_evidence=?,customer_confirmation=?,exception_reason=NULL WHERE id=?",
        )
        .bind(
          now,
          input.laborMinutes,
          input.evidence,
          input.customerConfirmation,
          id,
        ),
      write
        .prepare(
          "UPDATE field_service_part_reservations SET released_milli=quantity_milli-consumed_milli,release_reason='Visit completed' WHERE order_id=? AND quantity_milli>consumed_milli+released_milli",
        )
        .bind(id),
    );
  } else if (input.action === "release-billing") {
    const {invoiceDate,warrantyDiscountBps,warrantyApplied,amountMinor}=m.transition.billing!;
    steps.push(
      m.billing!.money,
      assertion(write,key,"completion","EXISTS(SELECT 1 FROM field_service_order_events WHERE order_id=? AND action='complete' AND actor_id<>?)",[id,a.userId]),
      assertion(write,key,"agreement","EXISTS(SELECT 1 FROM field_service_agreements WHERE id=? AND status='active')",[row.agreement_id]),
      ...m.billing!.steps,
      write
        .prepare(
          `INSERT INTO field_service_billing_releases(id,order_id,agreement_id,customer_id,currency,amount_minor,warranty_applied,warranty_discount_bps,warranty_end_snapshot,pricing_date,source_order_version,released_by,reason,created_at) SELECT ?,o.id,o.agreement_id,t.customer_id,?,?,?,?,?,?,?, ?,?,? FROM field_service_orders o JOIN (${supportProjectionSources.support_tickets}) t ON t.id=o.ticket_id WHERE o.id=?`,
        )
        .bind(
          `field-billing-${key}`,
          row.currency,
          amountMinor,
          warrantyApplied,
          warrantyDiscountBps,
          row.warranty_end ?? null,
          invoiceDate,
          input.expectedVersion + 1,
          a.userId,
          input.reason,
          now,
          id,
        ),
    );
  } else if(input.action==="cancel") {
    steps.push(
      assertion(write,key,'parts-gl-posted',`NOT EXISTS(SELECT 1 FROM (${inventoryProjectionSources.operational_inventory_consumptions}) p JOIN (${financeProjectionSources.finance_journals}) j ON j.id=p.journal_id WHERE p.source_kind='field_service' AND p.source_id=? AND j.status<>'posted')`,[id]),
      write
        .prepare(
          "UPDATE field_service_part_reservations SET released_milli=quantity_milli-consumed_milli,release_reason='Service order cancelled' WHERE order_id=? AND quantity_milli>consumed_milli+released_milli",
        )
        .bind(id),
    );
  }
  steps.push(
    write
      .prepare(
        "UPDATE field_service_orders SET status=?,technician_id=?,scheduled_at=?,exception_reason=?,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(status, tech, scheduled, exceptionReason, now, id),
    write
      .prepare(
        "INSERT INTO field_service_order_events VALUES(?,?,?,?,?,?,?,?,?,?::jsonb,?)",
      )
      .bind(
        `service-order-event-${key}`,
        id,
        input.expectedVersion + 1,
        input.action,
        a.userId,
        input.reason,
        status,
        tech,
        scheduled,
        JSON.stringify(details),
        now,
      ),
    prepareFieldServiceNotification(write,{key,companyId:a.companyId,technicianId:tech,orderId:id,version:input.expectedVersion+1,created:false,now}),
  );
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    input.expectedVersion + 1,
    result,
    steps,
  );
}
function application(a:WorkContext,provided?:SqlDatabase){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=holdingResources(a.holdingId).then(x=>x.write);
 const ports:FieldServicePorts<Consumption,Billing,Spares>={
 actorId:a.userId,canRead:hasPermission(a,"records.read")&&hasModuleAccess(a.modules,"field_service"),canReview:hasPermission(a,"records.review"),clock:{now:()=>Date.now()},
 async authorize(write){access(a,write);},async authorizeReview(){requirePermission(a,"records.review");},
 replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),workspace:async()=>readWorkspace(await database(),a),
 agreement:async(id)=>(await database()).prepare(`SELECT g.* FROM field_service_agreements g WHERE g.id=? AND ${aScope}`).bind(id,...args(a)).first<Agreement>(),
 order:async(id)=>{return (await database())
    .prepare(
      `SELECT o.*,t.customer_id,g.visit_rate_minor,g.warranty_discount_bps,g.currency,g.status agreement_status,a.warranty_end,CASE WHEN ${technician} THEN 1 ELSE 0 END technician_eligible,CASE WHEN ${reviewer("o")} THEN 1 ELSE 0 END can_review FROM field_service_orders o JOIN field_service_agreements g ON g.id=o.agreement_id JOIN (${supportProjectionSources.support_tickets}) t ON t.id=o.ticket_id LEFT JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=o.asset_id WHERE o.id=? AND ${oScope}`,
    )
    .bind(a.userId, id, ...args(a))
.first<OrderFacts>();},detail:async(order)=>readDetail(await database(),a,order),
 reservation:async(orderId,id)=>(await database()).prepare("SELECT * FROM field_service_part_reservations WHERE id=? AND order_id=?").bind(id,orderId).first<Row>(),
 prepareConsumption:async(row,reservation,input,key,now)=>{const write=await database(),effect=await prepareInventoryConsumption(write,a,{key,sourceKind:'field_service',sourceId:row.id,sourceVersion:input.expectedVersion+1,branchId:String(row.branch_id),departmentId:row.department_id?String(row.department_id):null,
 warehouseId:String(reservation.warehouse_id),productId:String(reservation.product_id),quantityMilli:input.quantityMilli,entryDate:input.entryDate??new Date(now).toISOString().slice(0,10),reason:input.reason,
 ownReservationMilli:Number(reservation.quantity_milli)-Number(reservation.consumed_milli)-Number(reservation.released_milli),allocations:input.allocations,
 beforeStock:[assertion(write,key,'part',"EXISTS(SELECT 1 FROM field_service_part_reservations WHERE id=? AND order_id=? AND quantity_milli-consumed_milli-released_milli>=? AND expires_at>?)",[input.reservationId,row.id,input.quantityMilli,now]),write.prepare('UPDATE field_service_part_reservations SET consumed_milli=consumed_milli+? WHERE id=?').bind(input.quantityMilli,input.reservationId)]});return {effect,id:effect.id,valueMinor:effect.valueMinor,journalId:effect.journalId};},
 prepareBilling:async(row,terms,key,now)=>prepareFieldServiceInvoice(await database(),a,{key,orderId:row.id,customerId:row.customer_id,currency:row.currency,amountMinor:terms.amountMinor,invoiceDate:terms.invoiceDate,dueDate:terms.dueDate,now}),
 commit:async(m,result)=>{const write=await database();if(m.action==="agreement")return commitAgreement(write,a,m,result);if(m.action==="create-order")return commitCreateOrder(write,a,m,result);return commitOrder(write,a,m,result);},
 };return createFieldServiceApplication(ports);
}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export const fieldServiceWorkspace=(a:WorkContext)=>invoke(()=>application(a).workspace());
export const fieldServiceDetail=(a:WorkContext,id:string,provided?:SqlDatabase)=>invoke(()=>application(a,provided).detail(id));
export const changeAgreement=(a:WorkContext,id:string,raw:unknown,key:string)=>invoke(()=>application(a).changeAgreement(id,raw,key));
export const createServiceOrder=(a:WorkContext,raw:unknown,key:string)=>invoke(()=>application(a).createOrder(raw,key));
export const changeServiceOrder=(a:WorkContext,id:string,raw:unknown,key:string)=>invoke(()=>application(a).changeOrder(id,raw,key));
export const fieldServiceWork=(a:WorkContext)=>invoke(()=>application(a).work());
