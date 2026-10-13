import {learningTalentSnapshotSources as identityaccessSnapshotSources} from '../../identity_access/public/learning-talent-snapshot-sources';
import {learningTalentSnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/learning-talent-snapshot-sources';
const identityaccessSources=identityaccessSnapshotSources();
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { Row } from "@/server/modules/commerce/public/commerce";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { hasPermission } from "@/lib/identity_access/permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { snapshotMoney } from "@/server/modules/finance/public/company-money";
import { ApiError } from "@/server/modules/runtime/public/http";
import { decimalToMinor } from "@/lib/finance/finance-money";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { createTalentWorkspace, createTalentCandidate as createTalentCandidateApplication, createTalentApplication as createTalentApplicationApplication, createTalentProposal as createTalentProposalApplication, createTalentOffer as createTalentOfferApplication, createChangeTalentRecord, createTalentDetail, createTalentExecutiveEvidence, createTalentWork, type TalentCommandPorts, type TalentCommand, type TalentMutation, type TalentExecutiveEvidenceMetric } from "../application/talent";
import type { TalentKind } from "../domain/talent";
export type { TalentExecutiveEvidenceMetric } from "../application/talent";
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "talent"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Talent.",
    );
  if (!hasPermission(a, "records.review"))
    throw new ApiError(
      403,
      "TALENT_PRIVACY_FORBIDDEN",
      "Candidate data requires Talent reviewer access.",
    );
}
const eligibleOwner = `EXISTS(SELECT 1 ${identityaccessSources.talent_adapters_talent_assignments_x_from} WHERE x.company_id=a.company_id AND x.user_id=a.owner_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=r.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=r.department_id)))`;

type DatabaseSource=SqlDatabase|(()=>Promise<SqlDatabase>);
const databaseSource=(source:DatabaseSource)=>typeof source==="function"?source:async()=>source;
function lazyDatabase(a:WorkContext){let resource:Promise<SqlDatabase>|null=null;return()=>resource??=holdingResources(a.holdingId).then(resources=>resources.write);}
function commandPorts(database:()=>Promise<SqlDatabase>):TalentCommandPorts{return{authorize:access,error:violation=>new ApiError(violation.status,violation.code,violation.message),now:Date.now,today:()=>new Date().toISOString().slice(0,10),replay:async(a,key,op,id,input)=>replayCommand(await database(),a,key,op,id,input),commit:async(command:TalentCommand,mutation:TalentMutation)=>executeCommand(await database(),command.actor,command.key,command.operation,command.input,command.id,command.version,command.result,mutation.native as SqlStatement[])};}
function recordSource(kind:TalentKind){const table=kind === "requisition"
        ? "talent_requisitions"
        : kind === "application"
          ? "talent_applications"
          : kind === "proposal"
            ? "talent_proposals"
            : "talent_offers",join=kind==="requisition"?"":kind==="application"?"JOIN talent_requisitions r ON r.id=t.requisition_id":"JOIN talent_applications ap ON ap.id=t.application_id JOIN talent_requisitions r ON r.id=ap.requisition_id",companyRef=kind==="proposal"?"ap":"t",placementRef=kind==="requisition"?"t":"r";return {table,join,companyRef,placementRef};}
export function nativeTalentWorkspace(source:DatabaseSource){const database=databaseSource(source);return createTalentWorkspace({authorize:access,
async requisitions(a){const write=await database();return (await write
        .prepare(
          "SELECT * FROM talent_requisitions WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC",
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>()).results;},
async candidates(a){const write=await database();return (await write
        .prepare(
          "SELECT id,legal_name,email,phone,retention_until,status,version,updated_at FROM talent_candidates WHERE company_id=? ORDER BY updated_at DESC",
        )
        .bind(a.companyId)
        .all<Row>()).results;},
async applications(a){const write=await database();return (await write
        .prepare(
          `SELECT a.*,r.job_title,c.legal_name candidate_name,CASE WHEN ${eligibleOwner} THEN 1 ELSE 0 END owner_eligible FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE a.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) ORDER BY a.updated_at DESC`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>()).results;},
async people(a){const write=await database();return (await write
        .prepare(
          `SELECT DISTINCT x.user_id id,COALESCE(p.legal_name,x.user_id) name ${identityaccessSources.talent_adapters_talent_assignments_x_from} LEFT ${peoplepayrollSources.talent_adapters_talent_hr_people_p_join} ON p.company_id=x.company_id AND p.user_id=x.user_id AND p.merged_into_id IS NULL WHERE x.company_id=? AND iam_assignment_active(x.id,?) AND iam_principal_active(x.user_id,x.company_id,?) ORDER BY 2`,
        )
        .bind(a.companyId, Date.now(), Date.now())
        .all<Row>()).results;}
});}
export async function talentWorkspace(a:WorkContext){return nativeTalentWorkspace(lazyDatabase(a))(a);}
export {createTalentRequisition} from "./requisition";
export function nativeTalentCandidate(source:DatabaseSource){const database=databaseSource(source);return createTalentCandidateApplication({...commandPorts(database),
async prepare(command,i){const write=await database(),{actor:a,key,id,now}=command;return {native:[
    write
      .prepare(
        "INSERT INTO talent_candidates VALUES(?,?,?,?,?,?,?,?,'active',?,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.legalName,
        i.email.toLowerCase(),
        i.phone ?? null,
        i.consentEvidence,
        now,
        i.retentionUntil,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO talent_candidate_events VALUES(?,?,1,'consent',?,'Candidate consent recorded','active',?)",
      )
      .bind(`talent-candidate-event-${key}`, id, a.userId, now),
  ]};},
});}
export async function createTalentCandidate(a:WorkContext,raw:unknown,key:string){return nativeTalentCandidate(lazyDatabase(a))(a,raw,key);}
export function nativeTalentApplication(source:DatabaseSource){const database=databaseSource(source);return createTalentApplicationApplication({...commandPorts(database),async currentRequisition(a,i){const write=await database();return await write
      .prepare(
        "SELECT * FROM talent_requisitions WHERE id=? AND company_id=? AND version=? AND status='open' AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)",
      )
      .bind(i.requisitionId, a.companyId, i.requisitionVersion,a.branchId,a.branchId,a.departmentId,a.departmentId)
      .first<Row>();},
async prepare(command,i){const write=await database(),{actor:a,key,id,now}=command;return {native:[
    assertion(write,key,"requisition-source","EXISTS(SELECT 1 FROM talent_requisitions WHERE id=? AND company_id=? AND version=? AND status='open' AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))",[i.requisitionId,a.companyId,i.requisitionVersion,a.branchId,a.branchId,a.departmentId,a.departmentId]),
    assertion(
      write,
      key,
      "candidate",
      "EXISTS(SELECT 1 FROM talent_candidates WHERE id=? AND company_id=? AND status='active' AND retention_until>=?)",
      [i.candidateId, a.companyId, new Date().toISOString().slice(0, 10)],
    ),
    assertion(
      write,
      key,
      "owner",
      `EXISTS(SELECT 1 ${identityaccessSources.talent_adapters_talent_assignments_from} WHERE company_id=? AND user_id=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?))`,
      [a.companyId, i.ownerId, now, now],
    ),
    write
      .prepare(
        "INSERT INTO talent_applications VALUES(?,?,?,?,?,'applied',?,?,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.requisitionId,
        i.requisitionVersion,
        i.candidateId,
        i.ownerId,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO talent_application_events VALUES(?,?,1,'apply',?,'Candidate applied to approved requisition','applied',?)",
      )
      .bind(`talent-app-event-${key}`, id, a.userId, now),
  ]};},
});}
export async function createTalentApplication(a:WorkContext,raw:unknown,key:string){return nativeTalentApplication(lazyDatabase(a))(a,raw,key);}
export function nativeTalentProposal(source:DatabaseSource){const database=databaseSource(source);return createTalentProposalApplication({...commandPorts(database),
async prepare(command,i){const write=await database(),{actor:a,key,id,now}=command;return {native:[
    assertion(
      write,
      key,
      "application",
      "EXISTS(SELECT 1 FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.version=? AND a.status='interview' AND a.owner_id=? AND c.status='active' AND c.retention_until>=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?))",
      [i.applicationId, a.companyId, i.applicationVersion, a.userId,new Date().toISOString().slice(0,10),a.branchId,a.branchId,a.departmentId,a.departmentId],
    ),
    write
      .prepare(
        "INSERT INTO talent_proposals VALUES(?,?,?,?,?,?,'submitted',?,NULL,1,?,?)",
      )
      .bind(
        id,
        i.applicationId,
        i.applicationVersion,
        i.recommendation,
        i.scoreBps,
        i.evidence,
        a.userId,
        now,
        now,
      ),
  ]};},
});}
export async function createTalentProposal(a:WorkContext,raw:unknown,key:string){return nativeTalentProposal(lazyDatabase(a))(a,raw,key);}
export function nativeTalentOffer(source:DatabaseSource){const database=databaseSource(source);return createTalentOfferApplication({...commandPorts(database),moneyUnits:decimalToMinor,async snapshot(a,id,i,amount){const write=await database();return await snapshotMoney(write,a,{sourceType:"talent_offer",sourceId:id,amountKind:"monthly_compensation",transactionDate:i.startDate,currency:i.currency,currencyScale:2,amountUnits:String(amount)});},
async prepare(command,i,amount,capturedMoney){const write=await database(),{actor:a,key,id,now}=command,money=capturedMoney as Awaited<ReturnType<typeof snapshotMoney>>;return {native:[
    assertion(
      write,
      key,
      "application",
      "EXISTS(SELECT 1 FROM talent_applications a JOIN talent_proposals p ON p.application_id=a.id JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.version=? AND a.status='selected' AND p.status='accepted' AND c.status='active' AND c.retention_until>=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?))",
      [i.applicationId, a.companyId, i.applicationVersion,new Date().toISOString().slice(0,10),a.branchId,a.branchId,a.departmentId,a.departmentId],
    ),
    write
      .prepare(
        "INSERT INTO talent_offers VALUES(?,?,?,?,?,?,?,?,?,'draft',?,NULL,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.applicationId,
        i.applicationVersion,
        i.currency,
        amount,
        i.startDate,
        i.expiresOn,
        i.terms,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO talent_offer_events VALUES(?,?,1,'create',?,'Offer drafted from accepted proposal','draft',?)",
      )
      .bind(`talent-offer-event-${key}`, id, a.userId, now),
    money.statement,
  ]};},
});}
export async function createTalentOffer(a:WorkContext,raw:unknown,key:string){return nativeTalentOffer(lazyDatabase(a))(a,raw,key);}
export function nativeTalentChanges(source:DatabaseSource){const database=databaseSource(source);return createChangeTalentRecord({...commandPorts(database),
async record(a,kind,id){const write=await database(),{table,join,companyRef,placementRef}=recordSource(kind);return await write
      .prepare(
        `SELECT t.* FROM ${table} t ${join} WHERE t.id=? AND ${companyRef}.company_id=? AND (?::text IS NULL OR ${placementRef}.branch_id=?) AND (?::text IS NULL OR ${placementRef}.department_id=?)`,
      )
      .bind(id,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId)
      .first<Row>();},
async candidate(a,applicationId){const write=await database();return await write.prepare("SELECT c.id,c.version,c.status,c.retention_until FROM talent_candidates c JOIN talent_applications a ON a.candidate_id=c.id AND a.company_id=c.company_id WHERE a.id=? AND c.company_id=?").bind(applicationId,a.companyId).first<{id:string;version:number;status:string;retention_until:string}>();},
async prepare(command,kind,i,facts){const write=await database(),{actor:a,key,id,now}=command,{table}=recordSource(kind),{row,rule,candidate}=facts;
const next=i.expectedVersion+1,sets=["status=?", "version=version+1", "updated_at=?"],bind:(string|number|null)[]=[rule[1], now];
if (i.action === "reassign") {
    sets.push("owner_id=?");
    bind.push(i.ownerId ?? null);
  }
if (["approve", "reject", "approve-offer"].includes(i.action)) {
    sets.push(kind === "offer" ? "approved_by=?" : "reviewed_by=?");
    bind.push(a.userId);
  }
if (
    kind === "proposal" &&
    ["accept-proposal", "reject-proposal"].includes(i.action)
  ) {
    sets.push("reviewed_by=?");
    bind.push(a.userId);
  }
if (i.action === "accept-offer") {
    sets.push("accepted_evidence=?");
    bind.push(i.acceptanceEvidence ?? null);
  }
bind.push(id);
const statements: SqlStatement[] = [
    assertion(
      write,
      key,
      "record",
      `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND status=?)`,
      [id, i.expectedVersion, rule[0]],
    ),
    ...(i.action === "reassign"
      ? [
          assertion(
            write,
            key,
            "owner",
            `EXISTS(SELECT 1 ${identityaccessSources.talent_adapters_talent_assignments_from} WHERE company_id=? AND user_id=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?))`,
            [a.companyId, i.ownerId!, now, now],
          ),
        ]
      : []),
    write
      .prepare(`UPDATE ${table} SET ${sets.join(",")} WHERE id=?`)
      .bind(...bind),
  ];
if(candidate)statements.unshift(assertion(write,key,"candidate-current","EXISTS(SELECT 1 FROM talent_candidates WHERE id=? AND company_id=? AND version=? AND status='active' AND retention_until>=?)",[candidate.id,a.companyId,candidate.version,facts.candidateDate]));
if (kind === "requisition")
    statements.push(
      write
        .prepare(
          "INSERT INTO talent_requisition_events VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          `talent-req-event-${key}`,
          id,
          next,
          i.action,
          a.userId,
          i.reason,
          rule[1],
          now,
        ),
    );
if (kind === "application")
    statements.push(
      write
        .prepare(
          "INSERT INTO talent_application_events VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          `talent-app-event-${key}`,
          id,
          next,
          i.action,
          a.userId,
          i.reason,
          rule[1],
          now,
        ),
    );
if (kind === "offer")
    statements.push(
      write
        .prepare("INSERT INTO talent_offer_events VALUES(?,?,?,?,?,?,?,?)")
        .bind(
          `talent-offer-event-${key}`,
          id,
          next,
          i.action,
          a.userId,
          i.reason,
          rule[1],
          now,
        ),
    );
if (kind === "application" && i.action === "select")
    statements.unshift(
      assertion(
        write,
        key,
        "proposal",
        "EXISTS(SELECT 1 FROM talent_proposals WHERE application_id=? AND status='accepted')",
        [id],
      ),
    );
if (kind === "offer" && i.action === "accept-offer") {
    statements.push(
      write
        .prepare(
          "UPDATE talent_applications SET status='accepted',version=version+1,updated_at=? WHERE id=? AND status='offered'",
        )
        .bind(now, row.application_id),
    );
    statements.push(
      write
        .prepare(
          "INSERT INTO talent_hire_handoffs(id,offer_id,company_id,branch_id,department_id,candidate_id,source_offer_version,status,created_at) SELECT ?,o.id,o.company_id,r.branch_id,r.department_id,a.candidate_id,?,'open',? FROM talent_offers o JOIN talent_applications a ON a.id=o.application_id JOIN talent_requisitions r ON r.id=a.requisition_id WHERE o.id=?",
        )
        .bind(`talent-hire-${key}`, next, now, id),
    );
  }
if (kind === "offer" && i.action === "send")
    statements.push(
      write
        .prepare(
          "UPDATE talent_applications SET status='offered',version=version+1,updated_at=? WHERE id=? AND status='selected'",
        )
        .bind(now, row.application_id),
    );
return {native:statements};
},
});}
export async function changeTalentRecord(a:WorkContext,kind:TalentKind,id:string,raw:unknown,key:string){return nativeTalentChanges(lazyDatabase(a))(a,kind,id,raw,key);}
export function nativeTalentDetail(source:DatabaseSource){const database=databaseSource(source);return createTalentDetail({authorize:access,error:violation=>new ApiError(violation.status,violation.code,violation.message),
async application(a,id){const write=await database();return await write
      .prepare(
        `SELECT a.*,r.job_title,r.branch_id,r.department_id,c.legal_name candidate_name,c.email candidate_email,c.retention_until,c.version candidate_version,CASE WHEN ${eligibleOwner} THEN 1 ELSE 0 END owner_eligible FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE a.id=? AND a.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)`,
      )
      .bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId)
      .first<Row>();},
async history(id){const write=await database();return (
      await write
        .prepare(
          "SELECT * FROM talent_application_events WHERE application_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>()
    ).results;},
async proposal(id){const write=await database();return await write
      .prepare("SELECT * FROM talent_proposals WHERE application_id=?")
      .bind(id)
      .first<Row>();},
async offer(id){const write=await database();return await write
      .prepare("SELECT * FROM talent_offers WHERE application_id=?")
      .bind(id)
      .first<Row>();},
async handoff(offerId){const write=await database(),offer={id:offerId};return await write
          .prepare("SELECT * FROM talent_hire_handoffs WHERE offer_id=?")
          .bind(offer.id)
          .first<Row>();},
});}
export async function talentDetail(a:WorkContext,id:string){return nativeTalentDetail(lazyDatabase(a))(a,id);}
export function nativeTalentExecutiveEvidence(db:SqlDatabase){return createTalentExecutiveEvidence({authorize:access,error:violation=>new ApiError(violation.status,violation.code,violation.message),requisition:(a,id)=>db.prepare(`SELECT r.id,r.job_title,r.target_start_date,r.status,r.version,r.updated_at,r.branch_id,r.department_id
      FROM talent_requisitions r WHERE r.id=? AND r.company_id=?
        AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)
        AND r.status='open' LIMIT 1`)
      .bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId)
      .first<Row>(),applications:(a,id,range)=>db.prepare(`SELECT a.id,a.requisition_id,a.status,a.version,a.updated_at,r.job_title,r.branch_id,r.department_id
    FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id
    WHERE a.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?)
      AND (?::text IS NULL OR r.department_id=?) AND a.updated_at>=? AND a.updated_at<?
    LIMIT 1`)
    .bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, range.from, range.toExclusive)
    .first<Row>()});}
export async function talentExecutiveEvidenceRow(db:SqlDatabase,a:WorkContext,metric:TalentExecutiveEvidenceMetric,recordId:string,range?:{from:number;toExclusive:number}){return nativeTalentExecutiveEvidence(db)(a,metric,recordId,range);}
export async function talentWork(a:WorkContext):Promise<import("@/lib/work_projection/work-item").WorkItem[]>{const database=lazyDatabase(a);return createTalentWork({authorize:access,async rows(a){const write=await database();return (
      await write
        .prepare(
          `SELECT a.*,r.job_title,c.legal_name candidate_name,CASE WHEN ${eligibleOwner} THEN 1 ELSE 0 END owner_eligible FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE a.company_id=? AND a.status NOT IN ('accepted','rejected','withdrawn') AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) ORDER BY a.updated_at DESC`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>()
    ).results;}})(a);}
