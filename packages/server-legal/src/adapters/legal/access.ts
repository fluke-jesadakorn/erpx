import {requireLegalItemPurchasedRead} from '@erpx/server-legal/adapters/purchases';
import {createLegalAccessApplication} from "@erpx/server-legal/application/access";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { ApiError } from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type { LegalItem, LegalKind } from "@erpx/server-legal/public/rules/model";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { productionFeatureGuard } from "@erpx/server-pms/public/hospitality-rollout";
export const tables: Record<LegalKind, string> = {
  request: "legal_requests",
  matter: "legal_matters",
  contract: "legal_contracts",
  obligation: "legal_obligations",
  compliance: "legal_compliance",
  license: "legal_licenses",
  corporate: "legal_corporate",
  ip: "legal_ip_assets",
  privacy: "legal_privacy",
  counsel: "legal_counsel",
  template: "legal_templates",
  rule: "legal_rules",
};
export function fail(
  message: string,
  code = "LEGAL_INVALID",
  status = 422,
): never {
  throw new ApiError(status, code, message);
}
export function visibility(a: WorkContext, alias = "i") {
  return {
    sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?) AND (${alias}.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=${alias}.id AND lm.user_id=?))${a.role==='legal_requester'?` AND ${alias}.kind='request' AND ${alias}.created_by=?`:''}`,
    values: [
      a.companyId,
      a.branchId,
      a.branchId,
      a.departmentId,
      a.departmentId,
      a.userId,
      ...(a.role==='legal_requester'?[a.userId]:[]),
    ],
  };
}
export async function enabled(db: SqlDatabase, a: WorkContext) {
  return !!(await db
    .prepare("SELECT 1 FROM legal_settings WHERE company_id=? AND enabled=true")
    .bind(a.companyId)
    .first());
}
function application(db:SqlDatabase,a:WorkContext){
 const scope=visibility(a);
 return createLegalAccessApplication({actor:a,require:permission=>requirePermission(a,permission),error:(status,code,message)=>new ApiError(status,code,message),repository:{
  visible:id=>db.prepare(`SELECT i.* FROM legal_items i WHERE i.id=? AND ${scope.sql}`).bind(id,...scope.values).first<Omit<LegalItem,"data">>(),
  enabled:()=>enabled(db,a),
  editor:async id=>Boolean(await db.prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? AND access='edit'").bind(id,a.userId).first()),
  data:(id,kind)=>db.prepare(`SELECT data FROM ${tables[kind]} WHERE item_id=?`).bind(id).first<{data:LegalItem["data"]}>(),
  eligible:async(id,scope,permission)=>Boolean(await db.prepare("SELECT 1 WHERE legal_person_eligible(?,?,?,?,?)").bind(a.companyId,id,scope.branch_id,scope.department_id,permission).first()),
 }});
}
export async function access(db:SqlDatabase,a:WorkContext,id:string,permission="legal.read"){const item=await application(db,a).access(id,permission);await requireLegalItemPurchasedRead(db,a,item);return item;}
export function lock(db: SqlDatabase, company: string) {
  return db
    .prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")
    .bind(`legal:${company}`);
}
export function guards(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
  version: number,
  key: string,
  edit = false,
  allowFrozen = false,
  preparation = false,
): SqlStatement[] {
  const v = visibility(a);
  return [
    db.prepare(
      "SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))",
    ),
    lock(db, a.companyId),
    ...(preparation&&!a.branchId&&!a.departmentId&&hasPermission(a,"legal.manage")?[assertion(db,key,"legal-preparation-only","EXISTS(SELECT 1 FROM legal_items WHERE id=? AND kind IN ('rule','template'))",[id])]:productionFeatureGuard(db,key,a.companyId,"legal")),
    ...(!allowFrozen
      ? [
          assertion(
            db,
            key,
            "legal-content-available",
            "NOT EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=? AND state IN ('executing','completed'))",
            [id],
          ),
        ]
      : []),
    assertion(
      db,
      key,
      "legal-enabled",
      "EXISTS(SELECT 1 FROM legal_settings WHERE company_id=? AND enabled=true)",
      [a.companyId],
    ),
    assertion(
      db,
      key,
      "legal-access",
      `EXISTS(SELECT 1 FROM legal_items i WHERE i.id=? AND i.version=? AND ${v.sql}${edit ? " AND EXISTS(SELECT 1 FROM legal_members em WHERE em.item_id=i.id AND em.user_id=? AND em.access='edit')" : ""})`,
      [id, version, ...v.values, ...(edit ? [a.userId] : [])],
    ),
  ];
}
export function eligible(db:SqlDatabase,a:WorkContext,id:string,scope:{branch_id:string|null;department_id:string|null}={branch_id:a.branchId,department_id:a.departmentId},permission:string|null=null){return application(db,a).eligible(id,scope,permission);}
export function personGuard(db:SqlDatabase,a:WorkContext,key:string,userId:string,scope:{branch_id:string|null;department_id:string|null}={branch_id:a.branchId,department_id:a.departmentId},permission:string|null=null){
  return assertion(db,key,`legal-person-${userId}-${permission??'member'}`,"legal_person_eligible(?,?,?,?,?)",[a.companyId,userId,scope.branch_id,scope.department_id,permission]);
}
export function event(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
  action: string,
  note: string,
  version: number,
) {
  return db
    .prepare(
      "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)",
    )
    .bind(crypto.randomUUID(), id, action, a.userId, note, version, Date.now());
}
