import 'server-only';
import { createHash } from 'node:crypto';
import { z } from 'zod';
import artifact from '@/data/release-artifact.json';
import { productionScope, legalProductionScope, allReleaseCriteria, criteriaForScope, type ReleaseScopeVersion, type GateState, type ReleaseCriterion } from '@/data/production-scope';
import { samePerson } from '@/lib/iam';
import type { WorkContext } from './authorization';
import type { SqlDatabase } from './runtime/types';
import { ApiError } from './http';
import { assertion, executeCommand, replayCommand } from './commands';
import { authorityGuard, companyAuthority } from './staff-policy';
import {prepareAttachmentBinding} from './attachments/service';
import {systemEvidenceInput} from './system-evidence';

const reason = z.string().trim().min(3).max(2000);
const criterionId = z.enum(allReleaseCriteria.map(row => row.id) as [ReleaseCriterion, ...ReleaseCriterion[]]);
const scopeVersionSchema = z.enum([productionScope.version, legalProductionScope.version]);
export function releaseRevisionTable(scope: ReleaseScopeVersion) {
  return scope === legalProductionScope.version ? 'company_legal_release_revisions' : 'company_release_revisions';
}
const evidenceSchema = z.object({
  criterionId,
  summary: reason,
  files: z.array(z.object({
    reference: z.string().trim().min(1).max(1000),
    sha256: z.string().regex(/^[a-f0-9]{64}$/),
  }).strict()).min(1).max(30),
}).strict();
export const releaseSubmitSchema = z.object({
  scopeVersion: scopeVersionSchema,
  softwareVersion: z.string().min(1).max(100),
  expectedConfigurationRevision: z.number().int().min(0),
  expectedSetupVersion: z.number().int().positive(),
  reason,
  evidence: z.array(evidenceSchema).min(1).max(allReleaseCriteria.length),
}).strict().superRefine((input, ctx) => {
  const required = criteriaForScope(input.scopeVersion);
  if (input.evidence.length !== required.length || new Set(input.evidence.map(row => row.criterionId)).size !== required.length ||
      input.evidence.some(row => !required.some(criterion => criterion.id === row.criterionId)))
    ctx.addIssue({ code: 'custom', message: 'Supply evidence for every release criterion exactly once.' });
});
export const releaseReviewSchema = z.object({
  expectedVersion: z.number().int().positive(),
  criterionId,
  state: z.enum(['pending', 'passed']),
  reason,
}).strict();
export const releaseRevokeSchema = z.object({ expectedVersion: z.number().int().positive(), reason }).strict();

type ReleaseRow = {
  id: string; company_id: string; scope_version: string; software_version: string;
  configuration_revision: number; setup_version: number; evidence_sha256: string;
  evidence_json: string; requested_by: string; version: number; revoked_at: number | null;
  reason: string; created_at: number; updated_at: number;
};
type ReviewRow = { criterion_id: string; state: GateState; reviewer_id: string; reason: string; reviewed_at: number; authority_current: boolean };

function releaseCurrencyPredicate(companyExpression: string) {
  // The scalar wrapper uses a parameter expression exactly once.
  return `(SELECT NOT EXISTS(SELECT 1 FROM finance_charts c WHERE c.company_id=scope_company.id AND c.base_currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM commerce_settings c WHERE c.company_id=scope_company.id AND c.currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM pos_settings c WHERE c.company_id=scope_company.id AND c.currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM pms_properties c WHERE c.company_id=scope_company.id AND c.config_json::jsonb->>'currency' IS DISTINCT FROM 'THB')
    AND NOT EXISTS(SELECT 1 FROM hr_pay_groups c WHERE c.company_id=scope_company.id AND c.status='active' AND (c.country_code<>'TH' OR c.currency<>'THB'))
    FROM (SELECT ${companyExpression}::text AS id) scope_company)`;
}

/** Code-owned SQL expressions only. Used by owner, API, queue and scheduled guards. */
export function acceptedReleasePredicate(companyExpression: string, scope: ReleaseScopeVersion = productionScope.version) {
  if (artifact.softwareVersion === 'unbuilt') return 'FALSE';
  const literal = (value: string) => `'${value.replaceAll("'", "''")}'`;
  const criteria = criteriaForScope(scope).map(row => `(${literal(row.id)},${literal(row.permission)})`).join(',');
  return `EXISTS(SELECT 1 FROM company_release_acceptances release
    JOIN ${releaseRevisionTable(scope)} revision ON revision.company_id=release.company_id
    JOIN company_business_acceptance setup ON setup.company_id=release.company_id
    WHERE release.company_id=${companyExpression} AND release.revoked_at IS NULL
    AND release.scope_version=${literal(scope)} AND release.software_version=${literal(artifact.softwareVersion)}
    AND release.configuration_revision=revision.revision AND setup.status='accepted' AND setup.version=release.setup_version
    AND release.id=(SELECT latest.id FROM company_release_acceptances latest WHERE latest.company_id=release.company_id AND latest.scope_version=release.scope_version ORDER BY latest.created_at DESC,latest.id DESC LIMIT 1)
    ${scope === productionScope.version ? `AND (SELECT count(*) FROM organization_nodes branch WHERE branch.company_id=release.company_id AND branch.kind='branch' AND branch.status='active')=1 AND ${releaseCurrencyPredicate('release.company_id')}` : ''}
    AND NOT EXISTS(SELECT 1 FROM (VALUES ${criteria}) AS required(id,permission) WHERE NOT EXISTS(
      SELECT 1 FROM company_release_gate_reviews gate JOIN assignments reviewer ON reviewer.id=gate.assignment_id
      WHERE gate.company_id=release.company_id AND gate.release_id=release.id AND gate.criterion_id=required.id
      AND gate.state='passed' AND gate.permission=required.permission AND reviewer.user_id=gate.reviewer_id
      AND reviewer.company_id=release.company_id AND reviewer.scope_kind='holding'
      AND iam_assignment_active(reviewer.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(reviewer.user_id,reviewer.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(reviewer.role,reviewer.company_id,required.permission)
      AND NOT iam_same_person(gate.reviewer_id,release.requested_by,release.company_id))))`;
}

async function requireReviewer(db: SqlDatabase, actor: WorkContext, permission: string) {
  if (actor.branchId || actor.departmentId || actor.scope.kind !== 'holding' ||
      !await companyAuthority(db, actor, permission))
    throw new ApiError(403, 'RELEASE_REVIEW_AUTHORITY_REQUIRED', 'Use an active Company assignment with the required review permission.');
}

export async function currentReleaseInputs(db: SqlDatabase, companyId: string, scope: ReleaseScopeVersion = productionScope.version) {
  const [revision, setup, branches, currency] = await Promise.all([
    db.prepare(`SELECT revision FROM ${releaseRevisionTable(scope)} WHERE company_id=?`).bind(companyId).first<{ revision: number }>(),
    db.prepare('SELECT version,status FROM company_business_acceptance WHERE company_id=?').bind(companyId).first<{ version: number; status: string }>(),
    db.prepare("SELECT count(*) count FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active'").bind(companyId).first<{ count: number }>(),
    db.prepare(`SELECT ${releaseCurrencyPredicate('?')} AS compatible`).bind(companyId).first<{ compatible: boolean }>(),
  ]);
  return {
    scopeVersion: scope,
    softwareVersion: artifact.softwareVersion,
    configurationRevision: Number(revision?.revision ?? 0),
    setupVersion: Number(setup?.version ?? 0),
    setupAccepted: setup?.status === 'accepted',
    establishmentCount: Number(branches?.count ?? 0),
    currencyCompatible: scope === legalProductionScope.version || currency?.compatible === true,
  };
}

function inputBlockers(row: ReleaseRow, inputs: Awaited<ReturnType<typeof currentReleaseInputs>>) {
  return [
    ...(row.revoked_at ? ['Release acceptance was revoked.'] : []),
    ...(inputs.softwareVersion === 'unbuilt' ? ['Use an isolated production release build for release acceptance.'] : []),
    ...(row.scope_version !== inputs.scopeVersion ? ['The release scope changed.'] : []),
    ...(row.software_version !== inputs.softwareVersion ? ['The software build changed.'] : []),
    ...(Number(row.configuration_revision) !== inputs.configurationRevision ? ['Material Company configuration changed.'] : []),
    ...(!inputs.setupAccepted || Number(row.setup_version) !== inputs.setupVersion ? ['Company setup needs current independent acceptance.'] : []),
    ...(inputs.scopeVersion === productionScope.version && inputs.establishmentCount !== 1 ? ['This release requires exactly one active establishment.'] : []),
    ...(!inputs.currencyCompatible ? ['Company settings must use THB and Thai payroll jurisdiction.'] : []),
  ];
}

/** Read-only status may be used by scope guards without granting review access. */
export async function companyReleaseStatus(db: SqlDatabase, companyId: string, releaseId?: string, scope: ReleaseScopeVersion = productionScope.version) {
  const row = await db.prepare(`SELECT * FROM company_release_acceptances WHERE company_id=? ${releaseId ? 'AND id=?' : 'AND scope_version=?'} ORDER BY created_at DESC,id DESC LIMIT 1`)
    .bind(companyId, releaseId ?? scope).first<ReleaseRow>();
  const selectedScope = row ? scopeVersionSchema.parse(row.scope_version) : scope;
  const inputs = await currentReleaseInputs(db, companyId, selectedScope);
  const criteria = criteriaForScope(selectedScope);
  if (!row) return { state: 'pending' as GateState, inputs, release: null, gates: criteria.map(criterion => ({ ...criterion, state: 'pending' as GateState, reviewerId: null as string | null, reason: null as string | null, reviewedAt: null as number | null })), blockers: [
    'No release has been submitted for independent review.',
    ...(inputs.softwareVersion==='unbuilt' ? ['Use an isolated production release build for release acceptance.'] : []),
    ...(!inputs.setupAccepted ? ['Company setup needs current independent acceptance.'] : []),
    ...(selectedScope === productionScope.version && inputs.establishmentCount!==1 ? ['This release requires exactly one active establishment.'] : []),
    ...(!inputs.currencyCompatible ? ['Company settings must use THB and Thai payroll jurisdiction.'] : []),
  ] };
  const latest = releaseId ? await db.prepare('SELECT id FROM company_release_acceptances WHERE company_id=? AND scope_version=? ORDER BY created_at DESC,id DESC LIMIT 1')
    .bind(companyId, selectedScope).first<{ id: string }>() : row;
  const reviews = (await db.prepare(`SELECT g.*,EXISTS(SELECT 1 FROM assignments a WHERE a.id=g.assignment_id
    AND a.company_id=g.company_id AND a.user_id=g.reviewer_id AND a.scope_kind='holding'
    AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(a.role,a.company_id,g.permission)
    AND NOT iam_same_person(g.reviewer_id,?,g.company_id)) authority_current
    FROM company_release_gate_reviews g WHERE g.company_id=? AND g.release_id=?`).bind(row.requested_by, companyId, row.id).all<ReviewRow>()).results;
  const gates = criteria.map(criterion => {
    const review = reviews.find(entry => entry.criterion_id === criterion.id);
    return { ...criterion, state: review?.authority_current ? review.state : 'pending' as GateState,
      reviewerId: review?.reviewer_id ?? null, reason: review?.reason ?? null, reviewedAt: review?.reviewed_at ?? null };
  });
  const blockers = [...(latest?.id === row.id ? [] : ['A newer release superseded this acceptance.']),
    ...inputBlockers(row, inputs), ...gates.filter(gate => gate.state !== 'passed').map(gate => `${gate.label}: independent acceptance pending.`)];
  return { state: (blockers.length ? 'pending' : 'passed') as GateState, inputs, release: {
    id: row.id, version: Number(row.version), requestedBy: row.requested_by, reason: row.reason,
    scopeVersion: row.scope_version, softwareVersion: row.software_version,
    configurationRevision: Number(row.configuration_revision), setupVersion: Number(row.setup_version),
    evidenceSha256: row.evidence_sha256, evidence: JSON.parse(row.evidence_json) as z.infer<typeof evidenceSchema>[],
    revokedAt: row.revoked_at, createdAt: row.created_at,
  }, gates, blockers };
}

export async function releaseAcceptance(db: SqlDatabase, actor: WorkContext, releaseId?: string, scope: ReleaseScopeVersion = productionScope.version) {
  // Reviewers can read all release evidence only under a Company review assignment.
  if (actor.branchId || actor.departmentId || actor.scope.kind !== 'holding')
    throw new ApiError(403, 'RELEASE_REVIEW_AUTHORITY_REQUIRED', 'Open the Company workspace.');
  const result = await companyReleaseStatus(db, actor.companyId, releaseId, scope);
  const permissions = [...new Set(criteriaForScope(result.inputs.scopeVersion).map(row => row.permission))];
  const allowed = await Promise.all(permissions.map(permission => companyAuthority(db, actor, permission)));
  if (!allowed.some(Boolean)) throw new ApiError(403, 'RELEASE_REVIEW_AUTHORITY_REQUIRED', 'A release review assignment is required.');
  if (releaseId && !result.release) throw new ApiError(404, 'RELEASE_NOT_FOUND', 'Release not found.');
  return result;
}

function inputsGuard(db: SqlDatabase, actor: WorkContext, key: string, revision: number, setupVersion: number, scope: ReleaseScopeVersion) {
  return [
    db.prepare(`INSERT INTO ${releaseRevisionTable(scope)}(company_id,revision) VALUES(?,0) ON CONFLICT DO NOTHING`).bind(actor.companyId),
    db.prepare(`SELECT revision FROM ${releaseRevisionTable(scope)} WHERE company_id=? FOR SHARE`).bind(actor.companyId),
    assertion(db, key, 'release-configuration', `EXISTS(SELECT 1 FROM ${releaseRevisionTable(scope)} WHERE company_id=? AND revision=?)`, [actor.companyId, revision]),
    assertion(db, key, 'release-setup', "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND status='accepted' AND version=?)", [actor.companyId, setupVersion]),
    ...(scope === productionScope.version ? [assertion(db, key, 'release-establishment', "(SELECT count(*) FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active')=1", [actor.companyId]),
    assertion(db, key, 'release-currency', releaseCurrencyPredicate('?'), [actor.companyId])] : []),
  ];
}

export async function submitReleaseAcceptance(db: SqlDatabase, actor: WorkContext, raw: unknown, key: string) {
  await requireReviewer(db, actor, 'organization.manage');
  const input = releaseSubmitSchema.parse(raw);
  const operation = 'release.acceptance.submit';
  const prior = await replayCommand(db, actor, key, operation, actor.companyId, input); if (prior) return prior;
  const current = await currentReleaseInputs(db, actor.companyId, input.scopeVersion);
  if (current.softwareVersion === 'unbuilt') throw new ApiError(409, 'RELEASE_BUILD_REQUIRED', 'Submit from an isolated production release build.');
  if (input.softwareVersion !== current.softwareVersion || input.expectedConfigurationRevision !== current.configurationRevision ||
    input.expectedSetupVersion !== current.setupVersion || !current.setupAccepted || (input.scopeVersion === productionScope.version && current.establishmentCount !== 1) || !current.currencyCompatible)
    throw new ApiError(409, 'RELEASE_INPUTS_CHANGED', 'Review the current build, Company setup and configuration before submitting.');
  const id = `release-${crypto.randomUUID()}`, now = Date.now();
  const evidence = JSON.stringify(input.evidence);
  const digest = createHash('sha256').update(evidence).digest('hex');
  const result = { id, version: 1, state: 'pending' as const };
  return executeCommand(db, actor, key, operation, input, actor.companyId, 1, result, [
    authorityGuard(db, key, actor, 'organization.manage'),
    ...inputsGuard(db, actor, key, current.configurationRevision, current.setupVersion, input.scopeVersion),
    db.prepare(`INSERT INTO company_release_acceptances(id,company_id,scope_version,software_version,configuration_revision,
      setup_version,evidence_sha256,evidence_json,requested_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`)
      .bind(id, actor.companyId, input.scopeVersion, input.softwareVersion, current.configurationRevision,
        current.setupVersion, digest, evidence, actor.userId, input.reason, now, now),
    db.prepare("INSERT INTO company_release_events(id,company_id,release_id,version,action,actor_id,detail_json,created_at) VALUES(?,?,?,1,'submitted',?,?,?)")
      .bind(`release-event-${key}`, actor.companyId, id, actor.userId, JSON.stringify({ reason: input.reason, evidenceSha256: digest }), now),
  ]);
}

export async function reviewReleaseAcceptance(db: SqlDatabase, actor: WorkContext, id: string, raw: unknown, key: string) {
  const input = releaseReviewSchema.parse(raw);
  const criterion = allReleaseCriteria.find(row => row.id === input.criterionId)!;
  await requireReviewer(db, actor, criterion.permission);
  const operation = 'release.acceptance.review';
  const prior = await replayCommand(db, actor, key, operation, id, input); if (prior) return prior;
  const row = await db.prepare('SELECT * FROM company_release_acceptances WHERE company_id=? AND id=?').bind(actor.companyId, id).first<ReleaseRow>();
  if (!row) throw new ApiError(404, 'RELEASE_NOT_FOUND', 'Release not found.');
  const scope = scopeVersionSchema.parse(row.scope_version);
  if (!criteriaForScope(scope).some(required => required.id === input.criterionId))
    throw new ApiError(422, 'RELEASE_CRITERION_SCOPE', 'Choose a criterion belonging to this release scope.');
  const latest = await db.prepare('SELECT id FROM company_release_acceptances WHERE company_id=? AND scope_version=? ORDER BY created_at DESC,id DESC LIMIT 1')
    .bind(actor.companyId, scope).first<{ id: string }>();
  if (latest?.id !== id) throw new ApiError(409, 'RELEASE_SUPERSEDED', 'Review the latest release submission.');
  if (samePerson(actor, row.requested_by)) throw new ApiError(403, 'INDEPENDENT_REVIEW_REQUIRED', 'A different person must review the submitted release evidence.');
  if (Number(row.version) !== input.expectedVersion) throw new ApiError(409, 'STALE_VERSION', 'Reload the release before reviewing it.');
  const inputs = await currentReleaseInputs(db, actor.companyId, scope);
  if (inputBlockers(row, inputs).length) throw new ApiError(409, 'RELEASE_INPUTS_CHANGED', 'Submit a new release after build, scope or configuration changes.');
  const version = input.expectedVersion + 1, now = Date.now();
  return executeCommand(db, actor, key, operation, input, id, version, { id, version, criterionId: input.criterionId, state: input.state }, [
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
  ]);
}

export async function revokeReleaseAcceptance(db: SqlDatabase, actor: WorkContext, id: string, raw: unknown, key: string) {
  await requireReviewer(db, actor, 'organization.manage');
  const input = releaseRevokeSchema.parse(raw), operation = 'release.acceptance.revoke';
  const prior = await replayCommand(db, actor, key, operation, id, input); if (prior) return prior;
  const version = input.expectedVersion + 1, now = Date.now();
  return executeCommand(db, actor, key, operation, input, id, version, { id, version, state: 'pending' as const }, [
    authorityGuard(db, key, actor, 'organization.manage'),
    db.prepare('SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE').bind(actor.companyId, id),
    assertion(db, key, 'release-version', 'EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND version=? AND revoked_at IS NULL)', [actor.companyId, id, input.expectedVersion]),
    db.prepare('UPDATE company_release_acceptances SET revoked_at=?,version=?,updated_at=? WHERE company_id=? AND id=?').bind(now, version, now, actor.companyId, id),
    db.prepare("INSERT INTO company_release_events(id,company_id,release_id,version,action,actor_id,detail_json,created_at) VALUES(?,?,?,?,'revoked',?,?,?)")
      .bind(`release-event-${key}`, actor.companyId, id, version, actor.userId, JSON.stringify(input), now),
  ]);
}

/** Add a reviewed sanitized artifact without changing any immutable release
 * snapshot or claiming that a business/provider criterion has passed. */
export async function bindReleaseEvidence(db:SqlDatabase,actor:WorkContext,id:string,raw:unknown,key:string){
 await requireReviewer(db,actor,'organization.manage');
 const input=systemEvidenceInput.parse(raw),operation='release.acceptance.evidence.bind';
 const prior=await replayCommand(db,actor,key,operation,id,input);if(prior)return prior;
 const row=await db.prepare('SELECT version,revoked_at FROM company_release_acceptances WHERE company_id=? AND id=?').bind(actor.companyId,id).first<{version:number;revoked_at:number|null}>();
 if(!row||row.revoked_at!==null)throw new ApiError(404,'RELEASE_NOT_FOUND','Release not found.');
 if(Number(row.version)!==input.expectedVersion)throw new ApiError(409,'STALE_VERSION','Reload the release before binding its sanitized evidence.');
 const version=input.expectedVersion+1,prepared=await prepareAttachmentBinding(db,actor,{
  attachmentSetIds:input.attachmentSetIds,
  bindingContract:{targetType:'company_release_acceptance',bindingType:'release_acceptance_evidence',contexts:[{purposeKey:'sanitized_release_evidence',componentKind:'action',componentRef:'company_release_acceptance:evidence'}]},
  targetId:id,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:`${id}:evidence:${version}`,
 });
 return executeCommand(db,actor,key,operation,input,id,version,{id,version,manifest:prepared.manifest},[
  authorityGuard(db,key,actor,'organization.manage'),
  assertion(db,key,'release-evidence-current','EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND version=? AND revoked_at IS NULL)',[actor.companyId,id,input.expectedVersion]),
  ...prepared.guards,
  db.prepare('UPDATE company_release_acceptances SET version=?,updated_at=? WHERE company_id=? AND id=?').bind(version,Date.now(),actor.companyId,id),
  ...prepared.steps,
 ]);
}
