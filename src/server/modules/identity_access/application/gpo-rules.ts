import type {WorkContext} from '../public/authorization';
import type {ProviderConnection} from '../public/iam-providers';
import type {DirectoryInventory} from '@/lib/directory-operations';
import type {GpoRulePreview,GpoRuleTarget,GpoRulesOverview} from '@/lib/gpo-rules';
import {gpoRuleInput,gpoRuleRemoval} from '@/lib/gpo-rules';
import {z} from 'zod';
import {hasPermission} from '@/lib/permissions';

import {BusinessRuleError,type Clock,type IdentifierSource} from '../../shared';
import {canManageGpoRules,presentGpoRule,gpoRuleMatchesPosition,tokenSchema,type GpoRuleRow,type EffectiveRuleRow,type Position} from '../domain/gpo-rules';
export interface GpoCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[]):Promise<Result>;}
export interface GpoRulesRepository<Effect>{readRules(value:string | null):Promise<GpoRuleRow[]>;
readEffectiveRules(value:string | null):Promise<EffectiveRuleRow[]>;
visiblePositions(companyId:string,branchId:string | null,departmentId:string | null):Promise<{ id: string }[]>;
scopedJobs(companyId:string,branchId:string | null,departmentId:string | null):Promise<GpoRulesOverview["jobs"][number][]>;
taskProgress(companyId:string,branchId:string | null,departmentId:string | null):Promise<{position_id:string;position_name:string;version:number;disposition:string|null;reason:string|null;mapping_count:number;verified_count:number;blocked_count:number}[]>;
taskEligible(companyId:string,positionId:string,expectedVersion:number):Promise<Record<string,unknown> | null>;
guardTaskVersion(key:string,companyId:string,positionId:string,expectedVersion:number):Effect;
prepareTaskDisposition(reason:string,version:number,now:number,companyId:string,positionId:string):Effect;
catalogSnapshot(companyId:string):Promise<{ snapshot: string } | null>;
targetName(companyId:string,target:GpoRuleTarget):Promise<{ name: string } | null>;
activeConnection(companyId:string,connectionId:string):Promise<ProviderConnection & { version: number } | null>;
directoryWriteState(companyId:string,connectionId:string):Promise<{ version: number; tested_version: number | null; write_enabled: boolean } | null>;
activePositions(companyId:string):Promise<Position[]>;
activeHolders(companyId:string):Promise<{ engagement_id: string; person_id: string; position_id: string; version: number; name: string; accounts: { connection: string; id: string }[] | string }[]>;
storePreview(token:string,companyId:string,userId:string,assignmentId:string,token2:string,expiresAt:number):Promise<void>;
deleteExpiredPreviews(companyId:string,now:number):Promise<void>;
reviewedPreview(token:string,companyId:string,userId:string,assignmentId:string):Promise<{ state_token: string; expires_at: number } | null>;
lockCatalog():Effect;
guardCatalogSnapshot(key:string,companyId:string,snapshot:string):Effect;
prepareLegacyPublishRevision(value:string,companyId:string,legacy_position_id:string,revision:number,connectionId:string,gpoId:string,userFilterGroupId:string,computerTargetOuId:string,version:string,value2:string,userId:string,reason:string,key:string,now:number):Effect;
prepareLegacyPublishPolicy(version:number,revision:number,connectionId:string,gpoId:string,userFilterGroupId:string,computerTargetOuId:string,now:number,companyId:string,legacy_position_id:string):Effect;
prepareRulePublish(id:string,companyId:string,kind:"position",id2:string,version:number,revision:number,connectionId:string,gpoId:string,userFilterGroupId:string,computerTargetOuId:string,userId:string,reason:string,now:number):Effect;

prepareLegacyRemovalRevision(value:string,companyId:string,legacy_position_id:string,revision:number,value2:string,userId:string,reason:string,key:string,now:number):Effect;
prepareLegacyRemovalPolicy(version:number,revision:number,now:number,companyId:string,legacy_position_id:string):Effect;
prepareRuleRemoval(version:number,revision:number,userId:string,reason:string,now:number,id:string,companyId:string):Effect;
guardRetryVersion(key:string,companyId:string,id:string,expectedVersion:number):Effect;
prepareRuleRetry(companyId:string,id:string):Effect;
prepareLegacyRuleRetry(companyId:string,id:string):Effect;}
export interface GpoRulesPorts<Effect>{repository:GpoRulesRepository<Effect>;clock:Clock;identifiers:IdentifierSource;hash(value:unknown):string;commands:GpoCommands<Effect>;requireFreshMfa(input:{userId:string;sessionId:string}):Promise<unknown>;readInventory(connection:ProviderConnection):Promise<DirectoryInventory>;organizationGuard(companyId:string,key:string):Promise<Effect>;}
export function createGpoRulesApplication<Effect>(ports:GpoRulesPorts<Effect>){const r=ports.repository;

function manage(actor: WorkContext) {
  if (!canManageGpoRules(actor)) throw new BusinessRuleError(403, "IT_AUTHORITY_REQUIRED", "Choose Company IT authority to publish organization GPO rules.");
}
async function mutate(actor: WorkContext) {
  manage(actor);
  if (!actor.authority?.sessionId) throw new BusinessRuleError(403, "FRESH_MFA_REQUIRED", "Verify MFA before changing Directory access.");
  await ports.requireFreshMfa({ userId: actor.userId, sessionId: actor.authority.sessionId });
}
async function readGpoRules(companyId?: string) {
  return (await r.readRules(companyId ?? null));
}
async function effectiveGpoRules(companyId?: string): Promise<EffectiveRuleRow[]> {
  return (await r.readEffectiveRules(companyId ?? null));
}
async function gpoRulesOverview(actor: WorkContext): Promise<GpoRulesOverview> {
  if (!["iam.read", "people.read", "people.manage", "organization.manage"].some(permission => hasPermission(actor, permission)))
    throw new BusinessRuleError(403, "GPO_READ_REQUIRED", "Organization or Directory read access is required.");
  const positions = (await r.visiblePositions(actor.companyId,actor.branchId,actor.departmentId));
  const visible = new Set(positions.map(row => row.id));
  const effective = (await effectiveGpoRules( actor.companyId)).filter(row => visible.has(row.position_id));
  const visibleRules = new Set(effective.map(row => row.id));
  const rules = (await readGpoRules( actor.companyId)).filter(row => row.active && ((!actor.branchId && !actor.departmentId) || visibleRules.has(row.id))).map(presentGpoRule);
  const jobs = (await r.scopedJobs(actor.companyId,actor.branchId,actor.departmentId));
  const taskRows = (await r.taskProgress(actor.companyId,actor.branchId,actor.departmentId));
  const tasks: GpoRulesOverview["tasks"] = taskRows.map(row => ({ positionId: row.position_id, positionName: row.position_name,
    version: row.version, reason: row.reason, mappingCount: row.mapping_count, verifiedCount: row.verified_count,
    status: row.disposition === 'not_required' ? 'not_required' : row.blocked_count ? 'blocked' : row.mapping_count && row.mapping_count === row.verified_count ? 'completed' : 'pending' }));
  const effectiveByPosition: GpoRulesOverview["effectiveByPosition"] = Object.fromEntries(positions.map(row => [row.id, []]));
  for (const row of effective) effectiveByPosition[row.position_id].push({ ...presentGpoRule(row), inherited: row.target_kind !== "position" });
  return { rules, effectiveByPosition, jobs, tasks, canManage: canManageGpoRules(actor) };
}
async function markPositionGpoNotRequired(actor: WorkContext, raw: unknown, key: string) {
  const input = z.object({ positionId: z.string().min(1), expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(1000) }).parse(raw);
  manage(actor);
  const operation = "organization.gpo.not_required";
  const prior = await ports.commands.replay( actor, key, operation, input.positionId, input); if (prior) return prior;
  const eligible = await r.taskEligible(actor.companyId,input.positionId,input.expectedVersion);
  if (!eligible) throw new BusinessRuleError(409, "GPO_TASK_CHANGED", "Reload the Position task before closing it; active mappings must be removed first.");
  const version = input.expectedVersion + 1;
  return ports.commands.commit( actor, key, operation, input, input.positionId, version,
    { positionId: input.positionId, version, status: "not_required" }, [
      r.guardTaskVersion(key,actor.companyId,input.positionId,input.expectedVersion),
      r.prepareTaskDisposition(input.reason,version,ports.clock.now(),actor.companyId,input.positionId),
    ]);
}
async function catalogSnapshot(companyId: string) { return (await r.catalogSnapshot(companyId))!.snapshot; }
async function targetName(companyId: string, target: GpoRuleTarget) {
  const result = await r.targetName(companyId,target);
  if (!result) throw new BusinessRuleError(422, "GPO_TARGET_INVALID", "Choose an active Level, Department or Position in this Company.");
  return result.name;
}
async function providerEvidence(actor: WorkContext, input: z.infer<typeof gpoRuleInput>) {
  const connection = await r.activeConnection(actor.companyId,input.connectionId);
  const state = await r.directoryWriteState(actor.companyId,input.connectionId);
  if (!connection || !state?.write_enabled || state.version !== state.tested_version) throw new BusinessRuleError(409, "DIRECTORY_WRITE_NOT_READY", "Choose an active, tested Directory connection with writes enabled.");
  const inventory = await ports.readInventory({ ...connection, config: typeof connection.config === "string" ? JSON.parse(connection.config) : connection.config });
  const objects = [[input.gpoId, "gpo"], [input.userFilterGroupId, "group"], [input.computerTargetOuId, "ou"]].map(([id, kind]) => {
    const object = inventory.objects.find(candidate => candidate.id === id && candidate.kind === kind);
    if (!object) throw new BusinessRuleError(422, "DIRECTORY_TARGET_INVALID", `Choose a current ${kind} from this connection.`);
    return { id: object.id, version: object.version };
  });
  return { connectionId: connection.id, connectionVersion: connection.version, stateVersion: state.version, objects };
}
async function impact(actor: WorkContext, input: z.infer<typeof gpoRuleInput> | z.infer<typeof gpoRuleRemoval>, removing: boolean) {
  manage(actor);
  const rules = await readGpoRules( actor.companyId), current = rules.find(rule => rule.id === input.id);
  if ((input.id && !current) || (current?.version ?? 0) !== input.expectedVersion || (removing && !current?.active))
    throw new BusinessRuleError(409, "GPO_RULE_CHANGED", "Reload the current GPO rule before reviewing this change.");
  const definition = input as z.infer<typeof gpoRuleInput>;
  const target = removing ? { kind: current!.target_kind, id: current!.target_id } : definition.target;
  if (target.kind !== "position") throw new BusinessRuleError(422, "GPO_TARGET_INVALID", "GPO rules can only target a Position.");
  if (current && !removing && (current.target_kind !== target.kind || current.target_id !== target.id)) throw new BusinessRuleError(422, "GPO_RULE_TARGET_IMMUTABLE", "Remove the existing assignment and add a rule at the new source.");
  const name = removing ? current!.target_name : await targetName( actor.companyId, target);
  const evidence = removing ? null : await providerEvidence( actor, definition);
  const candidate = removing ? { ...current!, active: false } : {
    ...current, id: current?.id ?? "new", company_id: actor.companyId, target_kind: target.kind, target_id: target.id,
    connection_id: definition.connectionId, gpo_id: definition.gpoId, user_filter_group_id: definition.userFilterGroupId,
    computer_target_ou_id: definition.computerTargetOuId, active: true,
  } as GpoRuleRow;
  if (!removing && rules.some(rule => rule.active && rule.id !== current?.id && rule.target_kind === target.kind && rule.target_id === target.id && rule.connection_id === candidate.connection_id && rule.gpo_id === candidate.gpo_id && rule.user_filter_group_id === candidate.user_filter_group_id && rule.computer_target_ou_id === candidate.computer_target_ou_id))
    throw new BusinessRuleError(409, "GPO_RULE_DUPLICATE", "This GPO mapping is already assigned to this source.");
  const after = [...rules.filter(rule => rule.id !== current?.id), candidate];
  const positions = (await r.activePositions(actor.companyId));
  const affected = positions.filter(position => gpoRuleMatchesPosition({ ...candidate, active: true }, position));
  const holders = (await r.activeHolders(actor.companyId))
    .map(holder => ({ ...holder, accounts: typeof holder.accounts === "string" ? JSON.parse(holder.accounts) as { connection: string; id: string }[] : holder.accounts }));
  const membershipSet = (catalog: GpoRuleRow[]) => new Set(holders.flatMap(holder => {
    const position = positions.find(item => item.id === holder.position_id);
    return position ? catalog.filter(rule => gpoRuleMatchesPosition(rule, position)).flatMap(rule => holder.accounts.filter(account => account.connection === rule.connection_id)
      .map(account => JSON.stringify([rule.connection_id, rule.user_filter_group_id, account.id]))) : [];
  }));
  const beforeSet = membershipSet(rules), afterSet = membershipSet(after);
  const result: Omit<GpoRulePreview, "token" | "expiresAt"> = { target, targetName: name, positions: affected.map(({ id, title }) => ({ id, title })),
    holders: holders.filter(holder => affected.some(position => position.id === holder.position_id)).map(holder => ({ personId: holder.person_id, engagementId: holder.engagement_id, name: holder.name, accountIds: [...new Set(holder.accounts.filter(account => account.connection === candidate.connection_id).map(account => account.id))] })),
    changes: { add: [...afterSet].filter(item => !beforeSet.has(item)), remove: [...beforeSet].filter(item => !afterSet.has(item)), retained: [...beforeSet].filter(item => afterSet.has(item)) },
    inheritedByFuturePositions: target.kind !== "position", endpointStatus: "unconfirmed" };
  const snapshot = await catalogSnapshot( actor.companyId);
  return { preview: { ...result, token: ports.hash({ input, result, snapshot, positions, holders, evidence }) }, snapshot, current, evidence };
}
async function issuePreview(actor: WorkContext, preview: Omit<GpoRulePreview, "expiresAt">): Promise<GpoRulePreview> {
  const token = ports.hash(ports.identifiers.next()), expiresAt = ports.clock.now() + 10 * 60_000;
  await r.storePreview(token,actor.companyId,actor.userId,actor.assignmentId,preview.token,expiresAt);
  await r.deleteExpiredPreviews(actor.companyId,ports.clock.now());
  return { ...preview, token, expiresAt };
}
async function validatePreview(actor: WorkContext, token: string, currentStateToken: string) {
  const preview = await r.reviewedPreview(token,actor.companyId,actor.userId,actor.assignmentId);
  if (!preview || Number(preview.expires_at) <= ports.clock.now()) throw new BusinessRuleError(409, "GPO_PREVIEW_EXPIRED", "This preview expired. Review the current impact again.");
  if (preview.state_token !== currentStateToken) throw new BusinessRuleError(409, "GPO_PREVIEW_CHANGED", "The organization or Directory impact changed. Review it again.");
}
async function previewGpoRule(actor: WorkContext, raw: unknown) {
  return issuePreview( actor, (await impact( actor, gpoRuleInput.parse(raw), false)).preview);
}
async function previewRemoveGpoRule(actor: WorkContext, raw: unknown) {
  return issuePreview( actor, (await impact( actor, gpoRuleRemoval.parse(raw), true)).preview);
}
async function publishGpoRule(actor: WorkContext, raw: unknown, key: string) {
  const parsed = gpoRuleInput.extend({ previewToken: tokenSchema }).parse(raw), { previewToken, ...input } = parsed;
  await mutate(actor);
  const id = input.id ?? `gpo-rule-${key}`, operation = "organization.gpo.publish";
  const prior = await ports.commands.replay( actor, key, operation, id, parsed); if (prior) return prior;
  const graphGuard = await ports.organizationGuard( actor.companyId, key);
  const result = await impact( actor, input, false);
  await validatePreview( actor, previewToken, result.preview.token);
  const version = input.expectedVersion + 1, revision = (result.current?.revision ?? 0) + 1, now = ports.clock.now();
  const statements = [r.lockCatalog(), graphGuard, r.guardCatalogSnapshot(key,actor.companyId,result.snapshot)];
  if (result.current?.legacy_position_id) {
    statements.push(r.prepareLegacyPublishRevision(`position-gpo-${key}`,actor.companyId,result.current.legacy_position_id,revision,input.connectionId,input.gpoId,input.userFilterGroupId,input.computerTargetOuId,result.evidence!.objects[0].version,JSON.stringify(result.preview.holders.map(holder => holder.personId)),actor.userId,input.reason,key,now));
    statements.push(r.prepareLegacyPublishPolicy(version,revision,input.connectionId,input.gpoId,input.userFilterGroupId,input.computerTargetOuId,now,actor.companyId,result.current.legacy_position_id));
  } else statements.push(r.prepareRulePublish(id,actor.companyId,input.target.kind,input.target.id,version,revision,input.connectionId,input.gpoId,input.userFilterGroupId,input.computerTargetOuId,actor.userId,input.reason,now));
  return ports.commands.commit( actor, key, operation, parsed, id, version, { id, version, revision, status: "pending", endpointStatus: "unconfirmed" }, statements);
}
async function removeGpoRule(actor: WorkContext, raw: unknown, key: string) {
  const parsed = gpoRuleRemoval.extend({ previewToken: tokenSchema }).parse(raw), { previewToken, ...input } = parsed;
  await mutate(actor);
  const operation = "organization.gpo.remove", prior = await ports.commands.replay( actor, key, operation, input.id, parsed); if (prior) return prior;
  const graphGuard = await ports.organizationGuard( actor.companyId, key), result = await impact( actor, input, true);
  await validatePreview( actor, previewToken, result.preview.token);
  const version = input.expectedVersion + 1, revision = result.current!.revision + 1, now = ports.clock.now();
  const statements = [r.lockCatalog(), graphGuard, r.guardCatalogSnapshot(key,actor.companyId,result.snapshot)];
  if (result.current!.legacy_position_id) {
    statements.push(r.prepareLegacyRemovalRevision(`position-gpo-unmap-${key}`,actor.companyId,result.current!.legacy_position_id,revision,JSON.stringify(result.preview.holders.map(holder => holder.personId)),actor.userId,input.reason,key,now));
    statements.push(r.prepareLegacyRemovalPolicy(version,revision,now,actor.companyId,result.current!.legacy_position_id));
  } else statements.push(r.prepareRuleRemoval(version,revision,actor.userId,input.reason,now,input.id,actor.companyId));
  return ports.commands.commit( actor, key, operation, parsed, input.id, version, { id: input.id, version, revision, active: false, queuedForReconciliation: true }, statements);
}
async function retryGpoRuleConfig(actor: WorkContext, raw: unknown, key: string) {
  await mutate(actor);
  const input = gpoRuleRemoval.parse(raw);
  return ports.commands.commit( actor, key, "organization.gpo.retry", input, input.id, input.expectedVersion, { id: input.id, status: "pending" }, [
    r.guardRetryVersion(key,actor.companyId,input.id,input.expectedVersion),
    r.prepareRuleRetry(actor.companyId,input.id),
    r.prepareLegacyRuleRetry(actor.companyId,input.id),
  ]);
}
return {readGpoRules,effectiveGpoRules,gpoRulesOverview,markPositionGpoNotRequired,previewGpoRule,previewRemoveGpoRule,publishGpoRule,removeGpoRule,retryGpoRuleConfig};}
