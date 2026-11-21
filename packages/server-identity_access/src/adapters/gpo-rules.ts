import { nativePrepare } from '@erpx/server-identity_access/public/native-identity-write';
import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import * as peopleGpo from '@erpx/server-people_payroll/public/gpo-sources';
import * as organizationGpo from '@erpx/server-organization/public/gpo-sources';
import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {SqlDatabase,SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {requireFreshMfa} from '@erpx/server-identity_access/public/iam-mfa';
import {readAutomationInventory} from '@erpx/server-identity_access/public/directory-provider';
import {organizationGuard} from '@erpx/server-organization/public/organization-graph-guard';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {ProviderConnection} from '@erpx/server-identity_access/public/iam-providers';

import type {GpoRuleTarget,GpoRulesOverview} from "@erpx/feature-organization/lib/gpo-rules";

import {type GpoRuleRow,type EffectiveRuleRow} from '@erpx/server-identity_access/domain/gpo-rules';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {createGpoRulesApplication,type GpoRulesRepository} from '@erpx/server-identity_access/application/gpo-rules';
// Snapshot ordering must stay identical to preview fingerprints.
const rulesSnapshotSql = `SELECT COALESCE(jsonb_agg(jsonb_build_array(id,target_kind,target_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,active) ORDER BY id),'[]'::jsonb)::text snapshot FROM organization_gpo_rules WHERE company_id=?`;
const hash=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
async function external<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function application(db:SqlDatabase){const repository:GpoRulesRepository<SqlStatement>={async readRules(value:string | null){return (await db.prepare(`SELECT r.*,COALESCE(l.name,n.display_name,p.title,r.target_id) target_name
    FROM organization_gpo_rules r
    LEFT ${organizationSources.gpo_rules_staff_levels_l_join} ON r.target_kind='level' AND l.id=r.target_id AND l.company_id=r.company_id
    LEFT ${organizationSources.gpo_rules_organization_nodes_n_join} ON r.target_kind='department' AND n.id=r.target_id AND n.company_id=r.company_id
    LEFT ${organizationSources.gpo_rules_organization_positions_p_join} ON r.target_kind='position' AND p.id=r.target_id AND p.company_id=r.company_id
    WHERE (?::text IS NULL OR r.company_id=?) ORDER BY r.id`).bind(value, value).all<GpoRuleRow>()).results;},
async readEffectiveRules(value:string | null){return (await db.prepare(`SELECT r.*,p.id position_id,COALESCE(l.name,n.display_name,direct.title,r.target_id) target_name
    ${organizationSources.gpo_rules_organization_positions_p_from} JOIN organization_gpo_rules r ON r.company_id=p.company_id AND r.active
      AND r.target_kind='position' AND r.target_id=p.id
    LEFT ${organizationSources.gpo_rules_staff_levels_l_join} ON r.target_kind='level' AND l.id=r.target_id AND l.company_id=r.company_id AND l.active=1
    LEFT ${organizationSources.gpo_rules_organization_nodes_n_join} ON r.target_kind='department' AND n.id=r.target_id AND n.company_id=r.company_id AND n.status='active'
    LEFT ${organizationSources.gpo_rules_organization_positions_direct_join} ON r.target_kind='position' AND direct.id=r.target_id AND direct.company_id=r.company_id AND direct.active=1
    WHERE p.active=1 AND (?::text IS NULL OR p.company_id=?)
      AND r.target_kind='position'
    ORDER BY p.id,r.id`).bind(value, value).all<EffectiveRuleRow>()).results;},
visiblePositions(companyId:string,branchId:string | null,departmentId:string | null){return organizationGpo.gpoVisiblePositions(db,companyId,branchId,departmentId);},
async scopedJobs(companyId:string,branchId:string | null,departmentId:string | null){return (await db.prepare(`SELECT j.id,j.rule_id,j.position_id,j.person_id,j.engagement_id,j.account_id,j.connection_id,
      j.revision,j.desired_action,j.status,j.version,j.last_error,j.directory_observed_at,j.endpoint_observed_at
    FROM position_gpo_jobs j ${organizationSources.gpo_rules_organization_positions_p_join} ON p.id=j.position_id AND p.company_id=j.company_id
    WHERE j.company_id=? AND p.active=1 AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
    ORDER BY j.updated_at DESC,j.id LIMIT 200`)
    .bind(companyId,branchId,branchId,departmentId,departmentId).all<GpoRulesOverview["jobs"][number]>()).results;},
async taskProgress(companyId:string,branchId:string | null,departmentId:string | null){return (await db.prepare(`SELECT t.position_id position_id,p.title position_name,t.version,t.disposition,t.reason,
      count(r.id)::int mapping_count,count(r.id) FILTER (WHERE r.directory_config_status='directory_applied')::int verified_count,
      count(r.id) FILTER (WHERE r.directory_config_status='blocked')::int blocked_count
    FROM position_gpo_tasks t ${organizationSources.gpo_rules_organization_positions_p_join} ON p.id=t.position_id AND p.company_id=t.company_id
    LEFT JOIN organization_gpo_rules r ON r.company_id=t.company_id AND r.target_kind='position' AND r.target_id=t.position_id AND r.active
    WHERE t.company_id=? AND p.active=1 AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
    GROUP BY t.position_id,p.title,t.version,t.disposition,t.reason ORDER BY p.title,t.position_id`)
    .bind(companyId,branchId,branchId,departmentId,departmentId)
    .all<{position_id:string;position_name:string;version:number;disposition:string|null;reason:string|null;mapping_count:number;verified_count:number;blocked_count:number}>()).results;},
async taskEligible(companyId:string,positionId:string,expectedVersion:number){return db.prepare(`SELECT 1 FROM position_gpo_tasks t ${organizationSources.gpo_rules_organization_positions_p_join} ON p.company_id=t.company_id AND p.id=t.position_id AND p.active=1
    WHERE t.company_id=? AND t.position_id=? AND t.version=? AND t.disposition IS NULL
      AND NOT EXISTS(SELECT 1 FROM organization_gpo_rules r WHERE r.company_id=t.company_id AND r.target_kind='position' AND r.target_id=t.position_id AND r.active)`)
    .bind(companyId,positionId,expectedVersion).first();},
guardTaskVersion(key:string,companyId:string,positionId:string,expectedVersion:number){return assertion(db,key,"gpo-task-version",`EXISTS(SELECT 1 FROM position_gpo_tasks t ${organizationSources.gpo_rules_organization_positions_p_join} ON p.company_id=t.company_id AND p.id=t.position_id AND p.active=1
        WHERE t.company_id=? AND t.position_id=? AND t.version=? AND t.disposition IS NULL
          AND NOT EXISTS(SELECT 1 FROM organization_gpo_rules r WHERE r.company_id=t.company_id AND r.target_kind='position' AND r.target_id=t.position_id AND r.active))`,
        [companyId,positionId,expectedVersion]);},
prepareTaskDisposition(reason:string,version:number,now:number,companyId:string,positionId:string){return nativePrepare("w_19994fa25d7d")
        .bind(reason,version,now,companyId,positionId);},
async catalogSnapshot(companyId:string){return db.prepare(rulesSnapshotSql).bind(companyId).first<{ snapshot: string }>();},
targetName(companyId:string,target:GpoRuleTarget){return organizationGpo.gpoTargetName(db,companyId,target);},
async activeConnection(companyId:string,connectionId:string){return db.prepare("SELECT id,provider,config,version FROM iam_connections WHERE company_id=? AND id=? AND status='active'").bind(companyId, connectionId).first<ProviderConnection & { version: number }>();},
async directoryWriteState(companyId:string,connectionId:string){return db.prepare("SELECT version,tested_version,write_enabled FROM people_directory_state WHERE company_id=? AND connection_id=?").bind(companyId, connectionId).first<{ version: number; tested_version: number | null; write_enabled: boolean }>();},
activePositions(companyId:string){return organizationGpo.gpoActivePositions(db,companyId);},
activeHolders(companyId:string){return peopleGpo.gpoActiveHolders(db,companyId);},
async storePreview(token:string,companyId:string,userId:string,assignmentId:string,token2:string,expiresAt:number){await nativePrepare("w_98045adcb931")
    .bind(token, companyId, userId, assignmentId, token2, expiresAt).run();},
async deleteExpiredPreviews(companyId:string,now:number){await nativePrepare("w_3873fc804594").bind(companyId, now).run();},
async reviewedPreview(token:string,companyId:string,userId:string,assignmentId:string){return db.prepare("SELECT state_token,expires_at FROM organization_gpo_previews WHERE token=? AND company_id=? AND user_id=? AND assignment_id=?")
    .bind(token, companyId, userId, assignmentId).first<{ state_token: string; expires_at: number }>();},
lockCatalog(){return db.prepare("SELECT pg_advisory_xact_lock(821039320)");},
guardCatalogSnapshot(key:string,companyId:string,snapshot:string){return assertion(db, key, "gpo-rule-catalog", `((${rulesSnapshotSql})::jsonb = ?::jsonb)`, [companyId, snapshot]);},
prepareLegacyPublishRevision(value:string,companyId:string,legacy_position_id:string,revision:number,connectionId:string,gpoId:string,userFilterGroupId:string,computerTargetOuId:string,version:string,value2:string,userId:string,reason:string,key:string,now:number){return nativePrepare("w_a2a12b460451").bind(value, companyId, legacy_position_id, revision, connectionId, gpoId, userFilterGroupId, computerTargetOuId, version, value2, userId, reason, key, now);},
prepareLegacyPublishPolicy(version:number,revision:number,connectionId:string,gpoId:string,userFilterGroupId:string,computerTargetOuId:string,now:number,companyId:string,legacy_position_id:string){return nativePrepare("w_b21f69afcfd0")
      .bind(version, revision, connectionId, gpoId, userFilterGroupId, computerTargetOuId, now, companyId, legacy_position_id);},
prepareRulePublish(id:string,companyId:string,kind:"position",id2:string,version:number,revision:number,connectionId:string,gpoId:string,userFilterGroupId:string,computerTargetOuId:string,userId:string,reason:string,now:number){return nativePrepare("w_8b6f1cdbeb4b")
    .bind(id, companyId, kind, id2, version, revision, connectionId, gpoId, userFilterGroupId, computerTargetOuId, userId, reason, now);},

prepareLegacyRemovalRevision(value:string,companyId:string,legacy_position_id:string,revision:number,value2:string,userId:string,reason:string,key:string,now:number){return nativePrepare("w_1926c148cab0")
      .bind(value, companyId, legacy_position_id, revision, value2, userId, reason, key, now);},
prepareLegacyRemovalPolicy(version:number,revision:number,now:number,companyId:string,legacy_position_id:string){return nativePrepare("w_0393992d21a0")
      .bind(version, revision, now, companyId, legacy_position_id);},
prepareRuleRemoval(version:number,revision:number,userId:string,reason:string,now:number,id:string,companyId:string){return nativePrepare("w_5d6ae0101eaf")
    .bind(version, revision, userId, reason, now, id, companyId);},
guardRetryVersion(key:string,companyId:string,id:string,expectedVersion:number){return assertion(db, key, "gpo-rule-version", "EXISTS(SELECT 1 FROM organization_gpo_rules WHERE company_id=? AND id=? AND version=? AND active AND directory_config_status='blocked')", [companyId, id, expectedVersion]);},
prepareRuleRetry(companyId:string,id:string){return nativePrepare("w_d33bb5678a53").bind(companyId, id);},
prepareLegacyRuleRetry(companyId:string,id:string){return nativePrepare("w_fd80373d4a38").bind(companyId, companyId, id);}};return createGpoRulesApplication({repository,clock:{now:()=>Date.now()},identifiers:{next:()=>randomUUID()},hash,commands:{replay:(...args)=>replayCommand(db,...args),commit:(...args)=>executeCommand(db,...args)},requireFreshMfa:input=>external(()=>requireFreshMfa(input)),readInventory:c=>external(()=>readAutomationInventory(c)),organizationGuard:(companyId,key)=>organizationGuard(db,companyId,key)});}
export {canManageGpoRules,gpoRuleMatchesPosition,gpoRuleSourceId} from '@erpx/server-identity_access/domain/gpo-rules';
export type {GpoRuleRow,EffectiveRuleRow} from '@erpx/server-identity_access/domain/gpo-rules';
export const newGpoRuleId=()=>`gpo-rule-${randomUUID()}`;
export async function readGpoRules(db: SqlDatabase,companyId?: string){return exposed(()=>application(db).readGpoRules(companyId));}
export async function effectiveGpoRules(db: SqlDatabase,companyId?: string){return exposed(()=>application(db).effectiveGpoRules(companyId));}
export async function gpoRulesOverview(db: PostgresDatabase,actor: WorkContext){return exposed(()=>application(db).gpoRulesOverview(actor));}
export async function markPositionGpoNotRequired(db: PostgresDatabase,actor: WorkContext,raw: unknown,key: string){return exposed(()=>application(db).markPositionGpoNotRequired(actor,raw,key));}
export async function previewGpoRule(db: PostgresDatabase,actor: WorkContext,raw: unknown){return exposed(()=>application(db).previewGpoRule(actor,raw));}
export async function previewRemoveGpoRule(db: PostgresDatabase,actor: WorkContext,raw: unknown){return exposed(()=>application(db).previewRemoveGpoRule(actor,raw));}
export async function publishGpoRule(db: PostgresDatabase,actor: WorkContext,raw: unknown,key: string){return exposed(()=>application(db).publishGpoRule(actor,raw,key));}
export async function removeGpoRule(db: PostgresDatabase,actor: WorkContext,raw: unknown,key: string){return exposed(()=>application(db).removeGpoRule(actor,raw,key));}
export async function retryGpoRuleConfig(db: PostgresDatabase,actor: WorkContext,raw: unknown,key: string){return exposed(()=>application(db).retryGpoRuleConfig(actor,raw,key));}
