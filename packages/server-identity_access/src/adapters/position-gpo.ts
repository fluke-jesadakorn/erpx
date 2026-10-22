import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
const peoplepayrollSources=peoplepayrollSnapshotSources();
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
import {readAutomationInventory,applyDirectoryAction} from '@erpx/server-identity_access/public/directory-provider';

import type {WorkContext} from '@erpx/server-identity_access/public/authorization';

import {type Connection,type Job,type Claim,type Ownership,type Desired} from '@erpx/server-identity_access/domain/position-gpo';

import {BusinessRuleError} from '@erpx/server-shared/index';
import {createPositionGpoApplication,type PositionGpoRepository} from '@erpx/server-identity_access/application/position-gpo';

const hash=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
async function external<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function application(db:SqlDatabase){const repository:PositionGpoRepository<SqlStatement>={async activeConnection(connectionId:string,companyId:string){return db.prepare("SELECT id,company_id,provider,config,status,version FROM iam_connections WHERE id=? AND company_id=? AND status='active'")
    .bind(connectionId,companyId).first<Connection>();},
async directoryWriteState(companyId:string,connectionId:string){return db.prepare("SELECT version,tested_version,write_enabled FROM people_directory_state WHERE company_id=? AND connection_id=?")
    .bind(companyId,connectionId).first<{version:number;tested_version:number|null;write_enabled:boolean}>();},
async publishingDirectoryState(id:string,companyId:string){return db.prepare("SELECT version,write_enabled,tested_version FROM people_directory_state WHERE connection_id=? AND company_id=?")
    .bind(id,companyId).first<{version:number;write_enabled:boolean;tested_version:number|null}>();},
activePosition(positionId:string,companyId:string){return organizationGpo.gpoActivePosition(db,positionId,companyId);},
async currentPolicy(companyId:string,positionId:string){return db.prepare("SELECT * FROM position_gpo_policies WHERE company_id=? AND position_id=?").bind(companyId,positionId).first<{version:number;revision:number;user_filter_group_id:string|null}>();},
positionHolders(connectionId:string,companyId:string,positionId:string){return peopleGpo.gpoPositionHolders(db,connectionId,companyId,positionId);},
async sharedMappings(companyId:string,connectionId:string,gpoId:string,positionId:string){return (await db.prepare(`SELECT p.position_id,p.revision,p.user_filter_group_id,p.computer_target_ou_id
    FROM position_gpo_policies p ${organizationSources.position_gpo_organization_positions_o_join} ON o.company_id=p.company_id AND o.id=p.position_id AND o.active=1
    WHERE p.company_id=? AND p.connection_id=? AND p.gpo_id=? AND p.position_id<>?
    ORDER BY p.position_id`).bind(companyId,connectionId,gpoId,positionId).all()).results;},
lockCatalog(){return db.prepare("SELECT pg_advisory_xact_lock(821039320)");},
guardItAuthority(key:string,assignmentId:string,userId:string,companyId:string){return assertion(db,key,"it-role","EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role='it_admin' AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(role,company_id,'iam.directory.apply'))",[assignmentId,userId,companyId]);},
guardPublishVersion(key:string,expectedVersion:number,companyId:string,positionId:string){return assertion(db,key,"gpo-version","(?=0 AND NOT EXISTS(SELECT 1 FROM position_gpo_policies WHERE company_id=? AND position_id=?)) OR EXISTS(SELECT 1 FROM position_gpo_policies WHERE company_id=? AND position_id=? AND version=?)",[expectedVersion,companyId,positionId,companyId,positionId,expectedVersion]);},
guardConnection(key:string,connectionId:string,companyId:string){return assertion(db,key,"gpo-connection","EXISTS(SELECT 1 FROM iam_connections c JOIN people_directory_state s ON s.connection_id=c.id WHERE c.id=? AND c.company_id=? AND c.status='active' AND s.write_enabled=true AND s.tested_version=s.version)",[connectionId,companyId]);},
preparePublishRevision(value:string,companyId:string,positionId:string,revision:number,connectionId:string,gpoId:string,userFilterGroupId:string,computerTargetOuId:string,version:string,value2:string,userId:string,reason:string,key:string,now:number){return db.prepare(`INSERT INTO position_gpo_revisions(id,company_id,position_id,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_gpo_version,affected_person_ids,published_by,reason,command_id,created_at)
      VALUES(?,?,?,?,?,?,?,?,?,?::jsonb,?,?,?,?)`).bind(value,companyId,positionId,revision,connectionId,gpoId,userFilterGroupId,computerTargetOuId,version,value2,userId,reason,key,now);},
preparePublishPolicy(companyId:string,positionId:string,version:number,revision:number,connectionId:string,gpoId:string,userFilterGroupId:string,computerTargetOuId:string,now:number){return db.prepare(`INSERT INTO position_gpo_policies(company_id,position_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_config_status,updated_at)
      VALUES(?,?,?,?,?,?,?,?,'pending',?) ON CONFLICT(company_id,position_id) DO UPDATE SET version=excluded.version,revision=excluded.revision,connection_id=excluded.connection_id,gpo_id=excluded.gpo_id,user_filter_group_id=excluded.user_filter_group_id,computer_target_ou_id=excluded.computer_target_ou_id,directory_config_status='pending',directory_config_error=NULL,updated_at=excluded.updated_at`)
      .bind(companyId,positionId,version,revision,connectionId,gpoId,userFilterGroupId,computerTargetOuId,now);},
async currentMapping(companyId:string,positionId:string){return db.prepare(`SELECT p.*,o.title FROM position_gpo_policies p ${organizationSources.position_gpo_organization_positions_o_join} ON o.id=p.position_id AND o.company_id=p.company_id
    WHERE p.company_id=? AND p.position_id=?`).bind(companyId,positionId)
    .first<{version:number;revision:number;title:string;connection_id:string|null;gpo_id:string|null;user_filter_group_id:string|null;computer_target_ou_id:string|null}>();},
unmapHolders(connection_id:string,companyId:string,positionId:string){return peopleGpo.gpoUnmapHolders(db,connection_id,companyId,positionId);},
async unmapSharedMappings(companyId:string,connection_id:string,gpo_id:string | null,positionId:string){return (await db.prepare(`SELECT p.position_id,p.revision,p.user_filter_group_id,p.computer_target_ou_id
    FROM position_gpo_policies p ${organizationSources.position_gpo_organization_positions_o_join} ON o.company_id=p.company_id AND o.id=p.position_id AND o.active=1
    WHERE p.company_id=? AND p.connection_id=? AND p.gpo_id=? AND p.position_id<>?
    ORDER BY p.position_id`).bind(companyId,connection_id,gpo_id,positionId).all()).results;},

guardUnmapVersion(key:string,companyId:string,positionId:string,expectedVersion:number){return assertion(db,key,"mapping-version","EXISTS(SELECT 1 FROM position_gpo_policies WHERE company_id=? AND position_id=? AND version=? AND connection_id IS NOT NULL)",[companyId,positionId,expectedVersion]);},
prepareUnmapRevision(value:string,companyId:string,positionId:string,revision:number,value2:string,userId:string,reason:string,key:string,now:number){return db.prepare(`INSERT INTO position_gpo_revisions(id,company_id,position_id,revision,connection_id,gpo_id,user_filter_group_id,
        computer_target_ou_id,directory_gpo_version,affected_person_ids,published_by,reason,command_id,created_at)
        VALUES(?,?,?,?,NULL,NULL,NULL,NULL,NULL,?::jsonb,?,?,?,?)`)
        .bind(value,companyId,positionId,revision,value2,userId,reason,key,now);},
prepareUnmapPolicy(version:number,revision:number,now:number,companyId:string,positionId:string){return db.prepare(`UPDATE position_gpo_policies SET version=?,revision=?,connection_id=NULL,gpo_id=NULL,user_filter_group_id=NULL,
        computer_target_ou_id=NULL,directory_config_status='directory_applied',directory_config_error=NULL,updated_at=?
        WHERE company_id=? AND position_id=?`).bind(version,revision,now,companyId,positionId);},
async overviewPolicies(companyId:string){return (await db.prepare(`SELECT p.*,p.connection_id IS NOT NULL mapped,o.title FROM position_gpo_policies p ${organizationSources.position_gpo_organization_positions_o_join} ON o.id=p.position_id AND o.company_id=p.company_id WHERE p.company_id=? ORDER BY o.title`).bind(companyId).all()).results;},
async overviewJobs(companyId:string){return (await db.prepare(`SELECT j.id,j.position_id,j.person_id,j.engagement_id,j.account_id,p.legal_name,j.revision,j.desired_action,j.status,j.version,j.attempts,j.last_error,j.directory_observed_at,j.endpoint_observed_at FROM position_gpo_jobs j ${peoplepayrollSources.position_gpo_hr_people_p_join} ON p.id=j.person_id WHERE j.company_id=? ORDER BY j.updated_at DESC LIMIT 200`).bind(companyId).all()).results;},
overviewPositions(companyId:string){return organizationGpo.gpoOverviewPositions(db,companyId);},
async currentJob(id:string,companyId:string){return db.prepare("SELECT company_id,version,status FROM position_gpo_jobs WHERE id=? AND company_id=?").bind(id,companyId).first<{company_id:string;version:number;status:string}>();},

guardJobVersion(key:string,id:string,companyId:string,expectedVersion:number){return assertion(db,key,"job-version","EXISTS(SELECT 1 FROM position_gpo_jobs WHERE id=? AND company_id=? AND version=? AND status IN ('blocked','retry'))",[id,companyId,expectedVersion]);},
prepareJobRetry(now:number,id:string){return db.prepare("UPDATE position_gpo_jobs SET status='queued',version=version+1,last_error=NULL,updated_at=? WHERE id=?").bind(now,id);},
async currentConfig(companyId:string,positionId:string){return db.prepare("SELECT version,revision,directory_config_status FROM position_gpo_policies WHERE company_id=? AND position_id=?").bind(companyId,positionId).first<{version:number;revision:number;directory_config_status:string}>();},

guardConfigVersion(key:string,companyId:string,positionId:string,expectedVersion:number){return assertion(db,key,"config-version","EXISTS(SELECT 1 FROM position_gpo_policies WHERE company_id=? AND position_id=? AND version=? AND directory_config_status='blocked')",[companyId,positionId,expectedVersion]);},
prepareConfigRetry(now:number,companyId:string,positionId:string){return db.prepare("UPDATE position_gpo_policies SET directory_config_status='pending',directory_config_error=NULL,updated_at=? WHERE company_id=? AND position_id=?").bind(now,companyId,positionId);},
async configOwnership(){return (await db.prepare("SELECT connection_id,gpo_id,kind,target_id,status FROM position_gpo_config_ownership").all<Ownership>()).results;},
async connectionCompany(connectionId:string){return db.prepare("SELECT company_id FROM iam_connections WHERE id=?").bind(connectionId).first<{company_id:string}>();},
async stageFilterOwnership(connectionId:string,gpoId:string,groupId:string,now:number){await db.prepare(`INSERT INTO position_gpo_config_ownership(connection_id,gpo_id,kind,target_id,created_at,status)
          VALUES(?,?,'filter_group',?,?,'pending') ON CONFLICT DO NOTHING`).bind(connectionId,gpoId,groupId,now).run();},
async markFilterWritten(connectionId:string,gpoId:string,groupId:string){await db.prepare(`UPDATE position_gpo_config_ownership SET status='written'
          WHERE connection_id=? AND gpo_id=? AND kind='filter_group' AND target_id=? AND status='pending'`).bind(connectionId,gpoId,groupId).run();},
async markFilterConfirmed(connectionId:string,gpoId:string,groupId:string){await db.prepare(`UPDATE position_gpo_config_ownership SET status='confirmed'
          WHERE connection_id=? AND gpo_id=? AND kind='filter_group' AND target_id=? AND status='written'`).bind(connectionId,gpoId,groupId).run();},
async recoverFilterConfirmed(connectionId:string,gpoId:string,target_id:string){await db.prepare("UPDATE position_gpo_config_ownership SET status='confirmed' WHERE connection_id=? AND gpo_id=? AND kind='filter_group' AND target_id=? AND status='written'")
          .bind(connectionId,gpoId,target_id).run();},
async removeFilterOwnership(connectionId:string,gpoId:string,target_id:string){await db.prepare("DELETE FROM position_gpo_config_ownership WHERE connection_id=? AND gpo_id=? AND kind='filter_group' AND target_id=?").bind(connectionId,gpoId,target_id).run();},
async stageLinkOwnership(connectionId:string,gpoId:string,ouId:string,now:number){await db.prepare(`INSERT INTO position_gpo_config_ownership(connection_id,gpo_id,kind,target_id,created_at,status)
          VALUES(?,?,'ou_link',?,?,'pending') ON CONFLICT DO NOTHING`).bind(connectionId,gpoId,ouId,now).run();},
async markLinkWritten(connectionId:string,gpoId:string,ouId:string){await db.prepare("UPDATE position_gpo_config_ownership SET status='written' WHERE connection_id=? AND gpo_id=? AND kind='ou_link' AND target_id=? AND status='pending'")
          .bind(connectionId,gpoId,ouId).run();},
async markLinkConfirmed(connectionId:string,gpoId:string,ouId:string){await db.prepare("UPDATE position_gpo_config_ownership SET status='confirmed' WHERE connection_id=? AND gpo_id=? AND kind='ou_link' AND target_id=? AND status='written'")
          .bind(connectionId,gpoId,ouId).run();},
async recoverLinkConfirmed(connectionId:string,gpoId:string,target_id:string){await db.prepare("UPDATE position_gpo_config_ownership SET status='confirmed' WHERE connection_id=? AND gpo_id=? AND kind='ou_link' AND target_id=? AND status='written'")
          .bind(connectionId,gpoId,target_id).run();},
async removeLinkOwnership(connectionId:string,gpoId:string,ouId:string){await db.prepare("DELETE FROM position_gpo_config_ownership WHERE connection_id=? AND gpo_id=? AND kind='ou_link' AND target_id=?").bind(connectionId,gpoId,ouId).run();},
async markRuleApplied(company_id:string,id:string,revision:number){await db.prepare("UPDATE organization_gpo_rules SET directory_config_status='directory_applied',directory_config_error=NULL WHERE company_id=? AND id=? AND revision=?").bind(company_id,id,revision).run();},
async markLegacyApplied(company_id:string,legacy_position_id:string,revision:number){await db.prepare("UPDATE position_gpo_policies SET directory_config_status='directory_applied',directory_config_error=NULL WHERE company_id=? AND position_id=? AND revision=?").bind(company_id,legacy_position_id,revision).run();},
async markRuleBlocked(value:string,company_id:string,id:string,revision:number){await db.prepare("UPDATE organization_gpo_rules SET directory_config_status='blocked',directory_config_error=? WHERE company_id=? AND id=? AND revision=?").bind(value,company_id,id,revision).run();},
async markLegacyBlocked(value:string,company_id:string,legacy_position_id:string,revision:number){await db.prepare("UPDATE position_gpo_policies SET directory_config_status='blocked',directory_config_error=? WHERE company_id=? AND position_id=? AND revision=?").bind(value,company_id,legacy_position_id,revision).run();},
async desiredMembershipFacts(){return (await db.prepare(`SELECT e.id engagement_id,e.company_id,e.position_id,e.person_id,e.version engagement_version,
      r.id rule_id,r.legacy_position_id,r.revision,r.connection_id,r.gpo_id,r.user_filter_group_id filter_group_id,r.computer_target_ou_id,i.external_id account_id
    ${peoplepayrollSources.position_gpo_hr_engagements_e_from} ${peoplepayrollSources.position_gpo_hr_people_person_join} ON person.id=e.person_id AND person.company_id=e.company_id
    ${organizationSources.position_gpo_organization_positions_position_join} ON position.id=e.position_id AND position.company_id=e.company_id AND position.active=1
    JOIN organization_gpo_rules r ON r.company_id=e.company_id AND r.active
      AND r.target_kind='position' AND r.target_id=position.id
    LEFT JOIN LATERAL (SELECT DISTINCT i.external_id FROM iam_members m JOIN iam_identities i ON i.id=m.identity_id
      WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active'
        AND i.company_id=e.company_id AND i.person_id=e.person_id AND i.connection_id=r.connection_id AND i.enabled=true) i ON true
    WHERE e.status='active' AND e.start_date<=CURRENT_DATE::text AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text)
      AND person.status='active' AND person.merged_into_id IS NULL`).all<Omit<Desired,"source_id">&{legacy_position_id:string|null}>()).results;},
async latestDelivery(company_id:string,position_id:string,source_id:string,account_id:string,rule_id:string,revision:number,connection_id:string,filter_group_id:string,desired_action:"add" | "remove"){return db.prepare(`SELECT status FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND source_id=? AND account_id=?
      AND rule_id=? AND revision=? AND connection_id=? AND filter_group_id=? AND desired_action=?
      AND status IN ('queued','running','retry','blocked','directory_applied') ORDER BY updated_at DESC LIMIT 1`)
      .bind(company_id,position_id,source_id,account_id,rule_id,revision,connection_id,filter_group_id,desired_action).first<{status:string}>();},
async ownedMembership(connection_id:string,filter_group_id:string,account_id:string,source_id:string){return db.prepare("SELECT 1 FROM people_directory_membership_sources WHERE connection_id=? AND group_id=? AND member_id=? AND source_id=?")
        .bind(connection_id,filter_group_id,account_id,source_id).first();},
async enqueueDelivery(id:string,company_id:string,position_id:string,person_id:string,engagement_id:string | null,account_id:string | null,revision:number,engagement_version:number,connection_id:string,gpo_id:string,filter_group_id:string,computer_target_ou_id:string,desired_action:"add" | "remove",value:"queued" | "awaiting_identity",now:number,rule_id:string,source_id:string){await db.prepare(`INSERT INTO position_gpo_jobs(id,company_id,position_id,person_id,engagement_id,account_id,revision,engagement_version,
    connection_id,gpo_id,filter_group_id,computer_target_ou_id,desired_action,status,created_at,updated_at,rule_id,source_id)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING`)
    .bind(id,company_id,position_id,person_id,engagement_id,account_id,revision,engagement_version,
      connection_id,gpo_id,filter_group_id,computer_target_ou_id,desired_action,value,now,now,rule_id,source_id).run();},
async membershipClaims(){return (await db.prepare(`SELECT c.* FROM organization_gpo_membership_claims c JOIN people_directory_membership_sources s
    ON s.connection_id=c.connection_id AND s.group_id=c.group_id AND s.member_id=c.member_id AND s.source_id=c.source_id`).all<Claim>()).results;},
async pendingAdds(){return (await db.prepare(`SELECT id,position_id,engagement_id,account_id,connection_id,filter_group_id,rule_id,source_id,revision
    FROM position_gpo_jobs WHERE desired_action='add' AND status IN ('queued','retry','blocked')`).all<Job>()).results;},
async supersedeAdd(now:number,id:string){await db.prepare("UPDATE position_gpo_jobs SET status='superseded',version=version+1,updated_at=? WHERE id=? AND status IN ('queued','retry','blocked')").bind(now,id).run();},
async waitingIdentities(){return (await db.prepare("SELECT id,company_id,engagement_id,position_id,rule_id,revision FROM position_gpo_jobs WHERE status='awaiting_identity'").all<Job>()).results;},
async supersedeWaiting(now:number,id:string){await db.prepare("UPDATE position_gpo_jobs SET status='superseded',version=version+1,updated_at=? WHERE id=? AND status='awaiting_identity'").bind(now,id).run();},
async finishDelivery(status:"directory_applied" | "superseded",value:number | null,now:number,id:string){await db.prepare("UPDATE position_gpo_jobs SET status=?,directory_observed_at=COALESCE(?,directory_observed_at),last_error=NULL,version=version+1,updated_at=? WHERE id=?")
    .bind(status,value,now,id).run();},
async recoverInterrupted(now:number,now2:number){await db.prepare("UPDATE position_gpo_jobs SET status='retry',version=version+1,last_error='WORKER_INTERRUPTED',updated_at=? WHERE status='running' AND updated_at<?").bind(now,now2).run();},
async dueDeliveries(limit:number){return (await db.prepare("SELECT * FROM position_gpo_jobs WHERE status IN ('queued','retry') ORDER BY updated_at,id LIMIT ?").bind(limit).all<Job>()).results;},
async claimDelivery(now:number,id:string,version:number){return db.prepare("UPDATE position_gpo_jobs SET status='running',attempts=attempts+1,version=version+1,updated_at=? WHERE id=? AND version=? AND status IN ('queued','retry') RETURNING id").bind(now,id,version).first();},
async currentMembership(connection_id:string,filter_group_id:string,account_id:string,source_id:string){return db.prepare("SELECT 1 FROM people_directory_membership_sources WHERE connection_id=? AND group_id=? AND member_id=? AND source_id=?").bind(connection_id,filter_group_id,account_id,source_id).first();},
async deliveryRule(company_id:string,rule_id:string,revision:number){return db.prepare("SELECT directory_config_status FROM organization_gpo_rules WHERE company_id=? AND id=? AND revision=? AND active").bind(company_id,rule_id,revision).first<{directory_config_status:string}>();},
async existingMembershipSource(connection_id:string,filter_group_id:string,account_id:string){return db.prepare("SELECT 1 FROM people_directory_membership_sources WHERE connection_id=? AND group_id=? AND member_id=? LIMIT 1").bind(connection_id,filter_group_id,account_id).first();},
async preserveExternalMembership(connection_id:string,filter_group_id:string,account_id:string,now:number){await db.prepare("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES(?,?,?,'external',?) ON CONFLICT DO NOTHING").bind(connection_id,filter_group_id,account_id,now).run();},
async otherMembershipSource(connection_id:string,filter_group_id:string,account_id:string,source_id:string){return db.prepare("SELECT 1 FROM people_directory_membership_sources WHERE connection_id=? AND group_id=? AND member_id=? AND source_id<>? LIMIT 1").bind(connection_id,filter_group_id,account_id,source_id).first();},
async deferReplacement(now:number,id:string){await db.prepare("UPDATE position_gpo_jobs SET status='retry',last_error='AWAITING_REPLACEMENT_SOURCE',version=version+1,updated_at=? WHERE id=?").bind(now,id).run();},
async blockDelivery(value:string,now:number,id:string){await db.prepare("UPDATE position_gpo_jobs SET status='blocked',last_error=?,version=version+1,updated_at=? WHERE id=?").bind(value,now,id).run();},
async recordClaim(job:Job,now:number){
  await db.batch([
    db.prepare("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(job.connection_id,job.filter_group_id,job.account_id,job.source_id,now),
    db.prepare(`INSERT INTO organization_gpo_membership_claims(connection_id,group_id,member_id,source_id,rule_id,company_id,position_id,engagement_id,person_id,revision,engagement_version,gpo_id,computer_target_ou_id)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(connection_id,group_id,member_id,source_id) DO UPDATE SET rule_id=excluded.rule_id,position_id=excluded.position_id,revision=excluded.revision,engagement_version=excluded.engagement_version,gpo_id=excluded.gpo_id,computer_target_ou_id=excluded.computer_target_ou_id`)
      .bind(job.connection_id,job.filter_group_id,job.account_id,job.source_id,job.rule_id,job.company_id,job.position_id,job.engagement_id,job.person_id,job.revision,job.engagement_version,job.gpo_id,job.computer_target_ou_id),
  ]);
},
async deleteClaim(job:Job){
  await db.batch([
    db.prepare("DELETE FROM people_directory_membership_sources WHERE connection_id=? AND group_id=? AND member_id=? AND source_id=?").bind(job.connection_id,job.filter_group_id,job.account_id,job.source_id),
    db.prepare("DELETE FROM organization_gpo_membership_claims WHERE connection_id=? AND group_id=? AND member_id=? AND source_id=?").bind(job.connection_id,job.filter_group_id,job.account_id,job.source_id),
  ]);
}};return createPositionGpoApplication({repository,clock:{now:()=>Date.now()},identifiers:{next:()=>randomUUID()},hash,commands:{replay:(...args)=>replayCommand(db,...args),commit:(...args)=>executeCommand(db,...args)},requireFreshMfa:input=>external(()=>requireFreshMfa(input)),readInventory:c=>external(()=>readAutomationInventory(c)),effectiveRules:()=>effectiveGpoRules(db),applyAction:(c,key,action,inventory)=>external(()=>applyDirectoryAction(c,key,action,inventory))});}
import {effectiveGpoRules} from '@erpx/server-identity_access/adapters/gpo-rules';
export {positionGpoInput} from '@erpx/server-identity_access/domain/position-gpo';
export async function previewPositionGpo(db:PostgresDatabase,actor:WorkContext,raw:unknown){return exposed(()=>application(db).previewPositionGpo(actor,raw));}
export async function publishPositionGpo(db:PostgresDatabase,actor:WorkContext,raw:unknown,key:string){return exposed(()=>application(db).publishPositionGpo(actor,raw,key));}
export async function previewUnmapPositionGpo(db:PostgresDatabase,actor:WorkContext,raw:unknown){return exposed(()=>application(db).previewUnmapPositionGpo(actor,raw));}
export async function unmapPositionGpo(db:PostgresDatabase,actor:WorkContext,raw:unknown,key:string){return exposed(()=>application(db).unmapPositionGpo(actor,raw,key));}
export async function positionGpoOverview(db:PostgresDatabase,actor:WorkContext){return exposed(()=>application(db).positionGpoOverview(actor));}
export async function retryPositionGpo(db:PostgresDatabase,actor:WorkContext,raw:unknown,key:string){return exposed(()=>application(db).retryPositionGpo(actor,raw,key));}
export async function retryPositionGpoConfig(db:PostgresDatabase,actor:WorkContext,raw:unknown,key:string){return exposed(()=>application(db).retryPositionGpoConfig(actor,raw,key));}
export async function reconcilePositionGpo(db:SqlDatabase){return exposed(()=>application(db).reconcilePositionGpo());}
export async function drainPositionGpo(db:PostgresDatabase,limit=10){
  // One pinned connection retains the session lock while short claim batches
  // commit independently. This also works with the production pool size of one.
  await db.withSession(async session=>{
    const locked=await session.prepare("SELECT pg_try_advisory_lock(821039320) locked").first<boolean>("locked");
    if(!locked)return;
    const connections:string[]=[];
    try{
      // Share the existing Directory worker's provider/source-ledger lock. Take
      // every relevant connection before any work so a busy provider defers the
      // tick without publishing partial configuration or failed delivery state.
      const relevant=(await session.prepare(`SELECT connection_id FROM organization_gpo_rules WHERE active
        UNION SELECT connection_id FROM position_gpo_config_ownership
        UNION SELECT connection_id FROM organization_gpo_membership_claims
        UNION SELECT connection_id FROM position_gpo_jobs WHERE status IN ('queued','retry','running')
        ORDER BY connection_id`).all<{connection_id:string}>()).results;
      for(const row of relevant){
        const acquired=await session.prepare("SELECT pg_try_advisory_lock(hashtext(?)) locked").bind("people-directory:"+row.connection_id).first<boolean>("locked");
        if(!acquired)return;
        connections.push(row.connection_id);
      }
      await exposed(()=>application(session).drainPositionGpoLocked(limit));
    }finally{
      for(const connectionId of connections.reverse())await session.prepare("SELECT pg_advisory_unlock(hashtext(?))").bind("people-directory:"+connectionId).run();
      await session.prepare("SELECT pg_advisory_unlock(821039320)").run();
    }
  });
}
