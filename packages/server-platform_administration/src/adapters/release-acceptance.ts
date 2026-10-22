import { platformOwnerProjectionSources as financePlatformSources } from "@erpx/server-finance/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as commercePlatformSources } from "@erpx/server-commerce/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as posPlatformSources } from "@erpx/server-pos/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as pmsPlatformSources } from "@erpx/server-pms/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/server-identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as organizationPlatformSources } from "@erpx/server-organization/public/platform-owner-projection-sources";
import "server-only";
import {createHash} from "node:crypto";
import artifact from "@erpx/catalogue/release-artifact.json";
import {productionScope,legalProductionScope,criteriaForScope,type ReleaseScopeVersion} from "@erpx/catalogue/production-scope";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {authorityGuard,companyAuthority} from "@erpx/server-operations/public/staff-policy";
import {prepareAttachmentBinding} from "@erpx/server-files/public/attachments/service";
import {createReleaseAcceptanceApplication,type ReleaseRepository} from "@erpx/server-platform_administration/application/release-acceptance";
import type {ReleaseRow,ReviewRow} from "@erpx/server-platform_administration/domain/release-acceptance";
export function releaseRevisionTable(scope: ReleaseScopeVersion) {
  return scope === legalProductionScope.version ? 'company_legal_release_revisions' : 'company_release_revisions';
}
function releaseCurrencyPredicate(companyExpression: string) {
  // The scalar wrapper uses a parameter expression exactly once.
  return `(SELECT NOT EXISTS(SELECT 1 FROM (${financePlatformSources.finance_charts}) c WHERE c.company_id=scope_company.id AND c.base_currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_settings}) c WHERE c.company_id=scope_company.id AND c.currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (${posPlatformSources.pos_settings}) c WHERE c.company_id=scope_company.id AND c.currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (${pmsPlatformSources.pms_properties}) c WHERE c.company_id=scope_company.id AND c.config_json::jsonb->>'currency' IS DISTINCT FROM 'THB')
    AND NOT EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_pay_groups}) c WHERE c.company_id=scope_company.id AND c.status='active' AND (c.country_code<>'TH' OR c.currency<>'THB'))
    FROM (SELECT ${companyExpression}::text AS id) scope_company)`;
}

/** Code-owned SQL expressions only. Used by owner, API, queue and scheduled guards. */
export function acceptedReleasePredicate(companyExpression: string, scope: ReleaseScopeVersion = productionScope.version) {
  if (artifact.softwareVersion === 'unbuilt') return 'FALSE';
  const literal = (value: string) => `'${value.replaceAll("'", "''")}'`;
  const criteria = criteriaForScope(scope).map(row => `(${literal(row.id)},${literal(row.permission)})`).join(',');
  return `EXISTS(SELECT 1 FROM company_release_acceptances release
    JOIN ${releaseRevisionTable(scope)} revision ON revision.company_id=release.company_id
    JOIN (${organizationPlatformSources.company_business_acceptance}) setup ON setup.company_id=release.company_id
    WHERE release.company_id=${companyExpression} AND release.revoked_at IS NULL
    AND release.scope_version=${literal(scope)} AND release.software_version=${literal(artifact.softwareVersion)}
    AND release.configuration_revision=revision.revision AND setup.status='accepted' AND setup.version=release.setup_version
    AND release.id=(SELECT latest.id FROM company_release_acceptances latest WHERE latest.company_id=release.company_id AND latest.scope_version=release.scope_version ORDER BY latest.created_at DESC,latest.id DESC LIMIT 1)
    ${scope === productionScope.version ? `AND (SELECT count(*) FROM (${organizationPlatformSources.organization_nodes}) branch WHERE branch.company_id=release.company_id AND branch.kind='branch' AND branch.status='active')=1 AND ${releaseCurrencyPredicate('release.company_id')}` : ''}
    AND NOT EXISTS(SELECT 1 FROM (VALUES ${criteria}) AS required(id,permission) WHERE NOT EXISTS(
      SELECT 1 FROM company_release_gate_reviews gate JOIN (${identity_accessPlatformSources.assignments}) reviewer ON reviewer.id=gate.assignment_id
      WHERE gate.company_id=release.company_id AND gate.release_id=release.id AND gate.criterion_id=required.id
      AND gate.state='passed' AND gate.permission=required.permission AND reviewer.user_id=gate.reviewer_id
      AND reviewer.company_id=release.company_id AND reviewer.scope_kind='holding'
      AND iam_assignment_active(reviewer.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(reviewer.user_id,reviewer.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(reviewer.role,reviewer.company_id,required.permission)
      AND NOT iam_same_person(gate.reviewer_id,release.requested_by,release.company_id))))`;
}

function inputsGuard(db: SqlDatabase, actor: WorkContext, key: string, revision: number, setupVersion: number, scope: ReleaseScopeVersion) {
  return [
    db.prepare(`INSERT INTO ${releaseRevisionTable(scope)}(company_id,revision) VALUES(?,0) ON CONFLICT DO NOTHING`).bind(actor.companyId),
    db.prepare(`SELECT revision FROM ${releaseRevisionTable(scope)} WHERE company_id=? FOR SHARE`).bind(actor.companyId),
    assertion(db, key, 'release-configuration', `EXISTS(SELECT 1 FROM ${releaseRevisionTable(scope)} WHERE company_id=? AND revision=?)`, [actor.companyId, revision]),
    assertion(db, key, 'release-setup', `EXISTS(SELECT 1 FROM (${organizationPlatformSources.company_business_acceptance}) company_business_acceptance WHERE company_id=? AND status='accepted' AND version=?)`, [actor.companyId, setupVersion]),
    ...(scope === productionScope.version ? [assertion(db, key, 'release-establishment', `(SELECT count(*) FROM (${organizationPlatformSources.organization_nodes}) organization_nodes WHERE company_id=? AND kind='branch' AND status='active')=1`, [actor.companyId]),
    assertion(db, key, 'release-currency', releaseCurrencyPredicate('?'), [actor.companyId])] : []),
  ];
}


function repository(db:SqlDatabase,companyId:string,work?:WorkContext):ReleaseRepository {
 const workActor=()=>{if(!work)throw new ApiError(403,'RELEASE_REVIEW_AUTHORITY_REQUIRED','Open the Company workspace.');return work;};
 return {
 inputFacts:async scope=>{const [revision,setup,branches,currency]=await Promise.all([
 db.prepare(`SELECT revision FROM ${releaseRevisionTable(scope)} WHERE company_id=?`).bind(companyId).first<{revision:number}>(),
 db.prepare(`SELECT version,status FROM (${organizationPlatformSources.company_business_acceptance}) company_business_acceptance WHERE company_id=?`).bind(companyId).first<{version:number;status:string}>(),
 db.prepare(`SELECT count(*) count FROM (${organizationPlatformSources.organization_nodes}) organization_nodes WHERE company_id=? AND kind='branch' AND status='active'`).bind(companyId).first<{count:number}>(),
 db.prepare(`SELECT ${releaseCurrencyPredicate('?')} AS compatible`).bind(companyId).first<{compatible:boolean}>()]);return {revision,setup,branches,currency};},
 row:(releaseId,scope)=>db.prepare(`SELECT * FROM company_release_acceptances WHERE company_id=? ${releaseId?'AND id=?':'AND scope_version=?'} ORDER BY created_at DESC,id DESC LIMIT 1`).bind(companyId,releaseId??scope).first<ReleaseRow>(),
 latest:scope=>db.prepare('SELECT id FROM company_release_acceptances WHERE company_id=? AND scope_version=? ORDER BY created_at DESC,id DESC LIMIT 1').bind(companyId,scope).first<{id:string}>(),
 reviews:async row=>(await db.prepare(`SELECT g.*,EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.assignments}) a WHERE a.id=g.assignment_id
    AND a.company_id=g.company_id AND a.user_id=g.reviewer_id AND a.scope_kind='holding'
    AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(a.role,a.company_id,g.permission)
    AND NOT iam_same_person(g.reviewer_id,?,g.company_id)) authority_current
    FROM company_release_gate_reviews g WHERE g.company_id=? AND g.release_id=?`).bind(row.requested_by, companyId, row.id).all<ReviewRow>()).results,
 replay:command=>replayCommand(db,workActor(),command.key,command.operation,command.id,command.input),
 submit:async(command,facts)=>{const {key,operation,input}=command,{id,now,current,evidence,digest}=facts,a=workActor(),actor=a,result={id,version:1,state:'pending' as const};return executeCommand(db, actor, key, operation, input, actor.companyId, 1, result, [
    authorityGuard(db, key, actor, 'organization.manage'),
    ...inputsGuard(db, actor, key, current.configurationRevision, current.setupVersion, input.scopeVersion),
    db.prepare(`INSERT INTO company_release_acceptances(id,company_id,scope_version,software_version,configuration_revision,
      setup_version,evidence_sha256,evidence_json,requested_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`)
      .bind(id, actor.companyId, input.scopeVersion, input.softwareVersion, current.configurationRevision,
        current.setupVersion, digest, evidence, actor.userId, input.reason, now, now),
    db.prepare("INSERT INTO company_release_events(id,company_id,release_id,version,action,actor_id,detail_json,created_at) VALUES(?,?,?,1,'submitted',?,?,?)")
      .bind(`release-event-${key}`, actor.companyId, id, actor.userId, JSON.stringify({ reason: input.reason, evidenceSha256: digest }), now),
  ]);},
 review:async(command,row,facts)=>{const {key,operation,id,input}=command,{inputs,scope,version,now}=facts,a=workActor(),actor=a,criterion={permission:facts.permission};return executeCommand(db, actor, key, operation, input, id, version, { id, version, criterionId: input.criterionId, state: input.state }, [
    authorityGuard(db, key, actor, criterion.permission),
    ...inputsGuard(db, actor, key, inputs.configurationRevision, inputs.setupVersion, scope),
    db.prepare('SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE').bind(actor.companyId, id),
    assertion(db, key, 'release-version', 'EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND version=? AND revoked_at IS NULL)', [actor.companyId, id, input.expectedVersion]),
    assertion(db, key, 'release-latest', 'EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND id=(SELECT latest.id FROM company_release_acceptances latest WHERE latest.company_id=? AND latest.scope_version=? ORDER BY latest.created_at DESC,latest.id DESC LIMIT 1))', [actor.companyId, id, actor.companyId, scope]),
    assertion(db, key, 'release-independent', 'NOT iam_same_person(?,?,?)', [actor.userId, row.requested_by, actor.companyId]),
    db.prepare(`INSERT INTO company_release_gate_reviews(company_id,release_id,criterion_id,state,reviewer_id,assignment_id,permission,reason,reviewed_at)
      VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(release_id,criterion_id) DO UPDATE SET state=excluded.state,reviewer_id=excluded.reviewer_id,
      assignment_id=excluded.assignment_id,permission=excluded.permission,reason=excluded.reason,reviewed_at=excluded.reviewed_at`)
      .bind(actor.companyId, id, input.criterionId, input.state, actor.userId, actor.assignmentId, criterion.permission, input.reason, now),
    db.prepare('UPDATE company_release_acceptances SET version=?,updated_at=? WHERE company_id=? AND id=?').bind(version, now, actor.companyId, id),
    db.prepare("INSERT INTO company_release_events(id,company_id,release_id,version,action,actor_id,detail_json,created_at) VALUES(?,?,?,?,'reviewed',?,?,?)")
      .bind(`release-event-${key}`, actor.companyId, id, version, actor.userId, JSON.stringify(input), now),
  ]);},
 revoke:async(command,facts)=>{const {key,operation,id,input}=command,{version,now}=facts,a=workActor(),actor=a;return executeCommand(db, actor, key, operation, input, id, version, { id, version, state: 'pending' as const }, [
    authorityGuard(db, key, actor, 'organization.manage'),
    db.prepare('SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE').bind(actor.companyId, id),
    assertion(db, key, 'release-version', 'EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND version=? AND revoked_at IS NULL)', [actor.companyId, id, input.expectedVersion]),
    db.prepare('UPDATE company_release_acceptances SET revoked_at=?,version=?,updated_at=? WHERE company_id=? AND id=?').bind(now, version, now, actor.companyId, id),
    db.prepare("INSERT INTO company_release_events(id,company_id,release_id,version,action,actor_id,detail_json,created_at) VALUES(?,?,?,?,'revoked',?,?,?)")
      .bind(`release-event-${key}`, actor.companyId, id, version, actor.userId, JSON.stringify(input), now),
  ]);},
 evidenceRow:id=>db.prepare('SELECT version,revoked_at FROM company_release_acceptances WHERE company_id=? AND id=?').bind(companyId,id).first<{version:number;revoked_at:number|null}>(),
 prepareEvidence:async(command,version)=>{const {key,operation,id,input}=command,a=workActor(),actor=a,prepared=await prepareAttachmentBinding(db,actor,{
  attachmentSetIds:input.attachmentSetIds,
  bindingContract:{targetType:'company_release_acceptance',bindingType:'release_acceptance_evidence',contexts:[{purposeKey:'sanitized_release_evidence',componentKind:'action',componentRef:'company_release_acceptance:evidence'}]},
  targetId:id,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:`${id}:evidence:${version}`,
 });return {commit:async now=>executeCommand(db,actor,key,operation,input,id,version,{id,version,manifest:prepared.manifest},[
  authorityGuard(db,key,actor,'organization.manage'),
  assertion(db,key,'release-evidence-current','EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND version=? AND revoked_at IS NULL)',[actor.companyId,id,input.expectedVersion]),
  ...prepared.guards,
  db.prepare('UPDATE company_release_acceptances SET version=?,updated_at=? WHERE company_id=? AND id=?').bind(version,now,actor.companyId,id),
  ...prepared.steps,
 ]) };},
 };}
function application(db:SqlDatabase,companyId:string,actor?:WorkContext){return createReleaseAcceptanceApplication({companyId,companyScope:Boolean(actor&&!actor.branchId&&!actor.departmentId&&actor.scope.kind==='holding'),samePerson:id=>Boolean(actor&&samePerson(actor,id)),authority:async permission=>Boolean(actor&&await companyAuthority(db,actor,permission)),softwareVersion:artifact.softwareVersion,clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash:value=>createHash('sha256').update(value).digest('hex'),repository:repository(db,companyId,actor),error:(status,code,message)=>new ApiError(status,code,message)});}
export function currentReleaseInputs(db:SqlDatabase,companyId:string,scope:ReleaseScopeVersion=productionScope.version){return application(db,companyId).currentReleaseInputs(scope);}
export function companyReleaseStatus(db:SqlDatabase,companyId:string,releaseId?:string,scope:ReleaseScopeVersion=productionScope.version){return application(db,companyId).companyReleaseStatus(releaseId,scope);}
export function releaseAcceptance(db:SqlDatabase,actor:WorkContext,releaseId?:string,scope:ReleaseScopeVersion=productionScope.version){return application(db,actor.companyId,actor).releaseAcceptance(releaseId,scope);}
export function submitReleaseAcceptance(db:SqlDatabase,actor:WorkContext,raw:unknown,key:string){return application(db,actor.companyId,actor).submitReleaseAcceptance(raw,key);}
export function reviewReleaseAcceptance(db:SqlDatabase,actor:WorkContext,id:string,raw:unknown,key:string){return application(db,actor.companyId,actor).reviewReleaseAcceptance(id,raw,key);}
export function revokeReleaseAcceptance(db:SqlDatabase,actor:WorkContext,id:string,raw:unknown,key:string){return application(db,actor.companyId,actor).revokeReleaseAcceptance(id,raw,key);}
export function bindReleaseEvidence(db:SqlDatabase,actor:WorkContext,id:string,raw:unknown,key:string){return application(db,actor.companyId,actor).bindReleaseEvidence(id,raw,key);}
