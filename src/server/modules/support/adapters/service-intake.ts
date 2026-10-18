import "server-only";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { requirePermission } from "../../identity_access/public/iam-permissions";
import { hasModuleAccess } from "@/data/module-access";
import { ApiError } from "../../runtime/public/http";
import { serviceIntakeAgreementGuards,type ServiceIntakeAgreement } from "../../field_service/public/intake-sources";
/** The Service command supplies its scoped purchase guards and commits these effects in that same batch. */
export function prepareServiceIntakeTicket(db:SqlDatabase,a:WorkContext,input:{key:string;orderId:string;agreement:ServiceIntakeAgreement;title:string;issue:string;technicianId:string;scheduledAt:number|null;now:number}) {
 requirePermission(a,"records.write");
 if(!hasModuleAccess(a.modules,"field_service"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Field Service.");
 const id=`ticket-${input.key}`,g=input.agreement,dueDate=new Date(input.scheduledAt??input.now).toISOString().slice(0,10);
 return { id,version:1,steps:[...serviceIntakeAgreementGuards(db,a,input.key,g),
 db.prepare(`INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,created_by,created_at,updated_at,service_agreement_id,service_order_id,service_order_version)
 VALUES(?,?,?,?,NULL,?,?,?,?,?,'open','field_service',?,?,?,?,?,1)`).bind(id,a.companyId,g.branch_id,g.department_id,g.customer_id,input.title,input.issue,input.technicianId,dueDate,a.userId,input.now,input.now,g.id,input.orderId),
 db.prepare("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) VALUES(?,?,1,'service-intake',?,?,?,?,'open',?,?::jsonb)")
 .bind(`ticket-event-${input.key}-intake`,id,a.userId,input.issue,input.technicianId,dueDate,input.now,JSON.stringify({serviceOrderId:input.orderId,agreementId:g.id,agreementVersion:g.version}))] };
}
