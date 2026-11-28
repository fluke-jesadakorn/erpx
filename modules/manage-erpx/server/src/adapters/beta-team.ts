import {betaManagementPeopleSources} from '@erpx/people-payroll-server/public/beta-management-sources';
import {betaManagementOrganizationSources} from '@erpx/organization-server/public/beta-management-sources';
import { iamDb } from "@erpx/identity-access-server/public/iam-db";
import { iamCommand } from "@erpx/identity-access-server/public/iam-transactions";
import { grantInput, proposeIamReview } from "@erpx/identity-access-server/public/iam-service";
import {validateIamGrant} from '@erpx/identity-access-server/public/iam-grants';
import { requireFreshMfa } from "@erpx/identity-access-server/public/iam-mfa";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { z } from "zod";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand} from "@erpx/workflow-server/public/commands";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import {
  authorityGuard,
  requireCompanyAuthority,
  type StaffActor,
} from "@erpx/operations-server/public/staff-policy";
import {
  branchStatements,
  memberStatements,
  type BetaUser,
} from "@erpx/manage-erpx-server/adapters/beta-onboarding";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { nativeOwnerWrite, NATIVE_OWNER_WRITE_NOOP_SQL } from "./native-owner-write";
import { betaPolicyPointSql } from "./beta-policy-point-sql";
import { nativeBetaInvitationAcceptedUser, nativeBetaInvitationByToken } from "./native-owner-read";
import { betaMemberCountSql } from "./beta-member-count-sql";
import { nativeWorkflowWrite } from "@erpx/workflow-server/public/native-workflow-evidence";

import type {SqlStatement} from '@erpx/platform-runtime-server/public/runtime/types';
import type {PoolClient} from 'pg';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {createBetaTeamApplication,type BetaTeamRepository,type BetaInvitationRepository} from '@erpx/manage-erpx-server/application/team';
import {invitationSchema} from '@erpx/manage-erpx-server/domain/team';
export {invitationSchema} from '@erpx/manage-erpx-server/domain/team';
import {betaInvitationSources as owner0} from '@erpx/organization-server/public/beta-invitation-sources';
import {betaInvitationSources as owner1} from '@erpx/people-payroll-server/public/beta-invitation-sources';
import {betaInvitationSources as owner2} from '@erpx/identity-access-server/public/beta-invitation-sources';
import {betaInvitationSources as owner3} from '@erpx/workflow-server/public/beta-invitation-sources';
async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function invitation(db:SqlDatabase,c:PoolClient):BetaInvitationRepository{const people=betaManagementPeopleSources(db),organization=betaManagementOrganizationSources(db);return {async capacity(companyId:string,now:number){return (await c.query(
          `SELECT (${betaMemberCountSql(people,'positional')})+(SELECT count(*) FROM beta_team_invitations i ${organization.invitationCompanyPoolSource} AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at>$2)<member_limit ok FROM beta_policy WHERE company_id=$1`,
          [companyId, now],
        )).rows[0]??null;},
// Native axum owner write executes the invitation insert in Rust.
async insertInvitation(id:string,email:string,branchId:string,userId:string,value:string,value2:number,now:number,role:"employee" | "it_admin"){
      await nativeOwnerWrite('me_beta_invitation_insert',[id,email,branchId,userId,value,value2,now,role],`me-invite:${id}`,{now},()=>db.prepare(NATIVE_OWNER_WRITE_NOOP_SQL));
      void c;
    },...owner0(db,c),...owner1(db,c),...owner2(db),...owner3(db),validateGrant:async(actor,grant)=>{await inward(()=>validateIamGrant(c,actor,grant));},review:(...args)=>inward(()=>proposeIamReview(c,...args))};}
function repository(db:SqlDatabase):BetaTeamRepository<SqlStatement>{const people=betaManagementPeopleSources(db),organization=betaManagementOrganizationSources(db);return {
// Native axum owner plans the byte-identical invitation point read; no TS SQL remains.
async invitation(value:string){return nativeBetaInvitationByToken(db,value);},
guardInvitation(key:string,value:"invite",id:string,value2:string,now:number){return assertion(
        db,
        key,
        value,
        "EXISTS(SELECT 1 FROM beta_team_invitations WHERE id=? AND email=? AND accepted_user_id IS NULL AND revoked_at IS NULL AND expires_at>?)",
        [id, value2, now],
      );},
guardCapacity(key:string,value:"capacity",company_id:string,company_id2:string){return assertion(
        db,
        key,
        value,
        `(${betaMemberCountSql(people,'bound')})<(${betaPolicyPointSql('member_limit')})`,
        [company_id, company_id2],
      );},
async prepareAcceptedInvitation(id:string,now:number,id2:string){return nativeOwnerWrite('me_beta_invitation_accept',[id,now,id2],`me-invite-accept:${id}`,{now},()=>db.prepare(NATIVE_OWNER_WRITE_NOOP_SQL));},
// Native axum owner plans the byte-identical accepted-user point read.
async acceptedUser(id:string){return nativeBetaInvitationAcceptedUser(db,id);},
guardBranchCapacity(key:string,value:"branch-capacity",companyId:string,companyId2:string){return assertion(
        db,
        key,
        value,
        `${organization.companyBranchCount}<(${betaPolicyPointSql('branch_limit')})`,
        [companyId, companyId2],
      );},...owner0(db),...owner1(db),...owner2(db),...owner3(db),prepareAcceptanceAudit(value:string,key:string,id:string,value2:string,id2:string,now:number){return nativeWorkflowWrite('wf_native_beta_invitation_sources_2',[value,key,id,value2,id2,now]);},async batch(effects){await db.batch(effects);},commit:(...args)=>executeCommand(db,...args),authorityGuard:(...args)=>authorityGuard(db,...args),memberStatements:(...args)=>memberStatements(db,...args),branchStatements:(...args)=>branchStatements(db,...args)};}
function application(db:SqlDatabase){return createBetaTeamApplication({parseGrant:input=>grantInput.parse(input),repository:repository(db),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash:sha256,requireCompanyAuthority:(...args)=>inward(()=>requireCompanyAuthority(db,...args)),requireFreshMfa:(...args)=>inward(()=>requireFreshMfa(...args)),invitationCommand:(actor,key,operation,input,permission,action)=>inward(()=>iamCommand(db as PostgresDatabase,actor,key,operation,input,permission,c=>action(invitation(db,c))))});}
export async function inviteMember(db: SqlDatabase,actor: StaffActor,rawInput: z.input<typeof invitationSchema>,key: string,token: string){return exposed(()=>application(db).inviteMember(actor,rawInput,key,token));}
export async function acceptMember(db: SqlDatabase,user: BetaUser,token: string){db=iamDb(db);return exposed(()=>application(db).acceptMember(user,token));}
export async function addBetaBranch(db: SqlDatabase,actor: StaffActor,name: string,key: string){return exposed(()=>application(db).addBetaBranch(actor,name,key));}

