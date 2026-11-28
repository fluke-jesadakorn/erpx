import {requireLegalItemPurchasedRead} from '@erpx/legal-server/adapters/purchases';
import {createLegalAccessApplication} from "@erpx/legal-server/application/access";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import type { LegalItem, LegalKind } from "@erpx/legal-server/public/rules/model";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { productionFeatureGuard } from "@erpx/pms-server/public/feature-availability";
import { legalEvent } from '@erpx/collaboration-server/adapters/native-legal-event';
import {legalCompanyAdvisoryLockSql,legalFilesAdvisoryLockSql,legalDestructionActiveSql,legalSettingsEnabledSql} from '../legal-read-tail-r9-sources';
import {legalItemSelectSql} from '../legal-read-tail-r16-sources';
import {legalMemberEditExistsSql} from '../legal-read-tail-r25-sources';
import {legalReleasedItemGuardSql,legalMemberEditPointSql,legalFactDataSql} from '../legal-read-tail-r38-sources';
import contract from '../legal-access-sources.json' with {type:'json'};
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token)) throw new Error('LEGAL_ACCESS_CONTRACT_INVALID');
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
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
    sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?) AND (${alias}.classification='internal' OR ${fill(contract.visibilityMemberGuard,{alias})})${a.role==='legal_requester'?` AND ${alias}.kind='request' AND ${alias}.created_by=?`:''}`,
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
    .prepare(`${legalSettingsEnabledSql}=true`)
    .bind(a.companyId)
    .first());
}
function application(db:SqlDatabase,a:WorkContext){
 const scope=visibility(a);
 return createLegalAccessApplication({actor:a,require:permission=>requirePermission(a,permission),error:(status,code,message)=>new ApiError(status,code,message),repository:{
  visible:id=>db.prepare(`${legalItemSelectSql}i.id=? AND ${scope.sql}`).bind(id,...scope.values).first<Omit<LegalItem,"data">>(),
  enabled:()=>enabled(db,a),
  editor:async id=>Boolean(await db.prepare(legalMemberEditPointSql('?')).bind(id,a.userId).first()),
  data:(id,kind)=>db.prepare(legalFactDataSql(tables[kind],'?')).bind(id).first<{data:LegalItem["data"]}>(),
  eligible:async(id,scope,permission)=>Boolean(await db.prepare(contract.eligibleProbe).bind(a.companyId,id,scope.branch_id,scope.department_id,permission).first()),
 }});
}
export async function access(db:SqlDatabase,a:WorkContext,id:string,permission="legal.read"){const item=await application(db,a).access(id,permission);await requireLegalItemPurchasedRead(db,a,item);return item;}
export function lock(db: SqlDatabase, company: string) {
  return db
    .prepare(legalCompanyAdvisoryLockSql)
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
    db.prepare(legalFilesAdvisoryLockSql),
    lock(db, a.companyId),
    ...(preparation&&!a.branchId&&!a.departmentId&&hasPermission(a,"legal.manage")?[assertion(db,key,"legal-preparation-only",contract.preparationGuard,[id])]:productionFeatureGuard(db,key,a.companyId,"legal")),
    ...(!allowFrozen
      ? [
          assertion(
            db,
            key,
            "legal-content-available",
            `NOT EXISTS(${legalDestructionActiveSql})`,
            [id],
          ),
        ]
      : []),
    assertion(
      db,
      key,
      "legal-enabled",
      `EXISTS(${legalSettingsEnabledSql}=true)`,
      [a.companyId],
    ),
    assertion(
      db,
      key,
      "legal-access",
      `EXISTS(${legalReleasedItemGuardSql('version')} AND ${v.sql}${edit ? " AND "+legalMemberEditExistsSql("em") : ""})`,
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
  return legalEvent(db, a.companyId, a.userId).record(id, action, note, version, crypto.randomUUID(), Date.now());
}
