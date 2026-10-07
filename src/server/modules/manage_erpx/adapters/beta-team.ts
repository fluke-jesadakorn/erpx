import {betaManagementPeopleSources} from '../../people_payroll/public/beta-management-sources';
import {betaManagementOrganizationSources} from '../../organization/public/beta-management-sources';
import { iamDb } from "@/server/modules/identity_access/public/iam-db";
import { iamCommand } from "@/server/modules/identity_access/public/iam-transactions";
import { grantInput, validateIamGrant, proposeIamReview } from "@/server/modules/identity_access/public/iam-service";
import { requireFreshMfa } from "@/server/modules/identity_access/public/iam-mfa";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { z } from "zod";
import { assertion, executeCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import {
  authorityGuard,
  requireCompanyAuthority,
  type StaffActor,
} from "@/server/modules/operations/public/staff-policy";
import {
  branchStatements,
  memberStatements,
  type BetaUser,
} from "@/server/beta-onboarding";
import { sha256 } from "@/server/modules/runtime/public/crypto";

import type {SqlStatement} from '../../runtime/public/runtime/types';
import type {PoolClient} from 'pg';
import {BusinessRuleError} from '../../shared';
import {createBetaTeamApplication,type BetaTeamRepository,type BetaInvitationRepository} from '../application/team';
import {invitationSchema} from '../domain/team';
export {invitationSchema} from '../domain/team';
import {betaInvitationSources as owner0} from '../../organization/public/beta-invitation-sources';
import {betaInvitationSources as owner1} from '../../people_payroll/public/beta-invitation-sources';
import {betaInvitationSources as owner2} from '../../identity_access/public/beta-invitation-sources';
import {betaInvitationSources as owner3} from '../../workflow/public/beta-invitation-sources';
async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function invitation(db:SqlDatabase,c:PoolClient):BetaInvitationRepository{const people=betaManagementPeopleSources(db),organization=betaManagementOrganizationSources(db);return {async capacity(companyId:string,now:number){return (await c.query(
          `SELECT (SELECT count(*) FROM beta_members m ${people.memberCompanyPoolSource})+(SELECT count(*) FROM beta_team_invitations i ${organization.invitationCompanyPoolSource} AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at>$2)<member_limit ok FROM beta_policy WHERE company_id=$1`,
          [companyId, now],
        )).rows[0]??null;},
async insertInvitation(id:string,email:string,branchId:string,userId:string,value:string,value2:number,now:number,role:"employee" | "it_admin"){await c.query(
        "INSERT INTO beta_team_invitations(id,email,branch_id,invited_by,token_hash,expires_at,created_at,role) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        [
          id,
          email,
          branchId,
          userId,
          value,
          value2,
          now,
          role,
        ],
      );},...owner0(db,c),...owner1(db,c),...owner2(db),...owner3(db),validateGrant:async(actor,grant)=>{await inward(()=>validateIamGrant(c,actor,grant));},review:(...args)=>inward(()=>proposeIamReview(c,...args))};}
function repository(db:SqlDatabase):BetaTeamRepository<SqlStatement>{const people=betaManagementPeopleSources(db),organization=betaManagementOrganizationSources(db);return {async invitation(value:string){return db
    .prepare("SELECT * FROM beta_team_invitations WHERE token_hash=?")
    .bind(value)
    .first<{
      id: string;
      email: string;
      branch_id: string;
      invited_by: string;
      expires_at: number;
      accepted_user_id: string | null;
      revoked_at: number | null;
      role: "employee" | "it_admin";
    }>();},
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
        `(SELECT count(*) FROM beta_members m ${people.memberCompanySource})<(SELECT member_limit FROM beta_policy WHERE company_id=?)`,
        [company_id, company_id2],
      );},
prepareAcceptedInvitation(id:string,now:number,id2:string){return db
        .prepare(
          "UPDATE beta_team_invitations SET accepted_user_id=?,accepted_at=? WHERE id=?",
        )
        .bind(id, now, id2);},
async acceptedUser(id:string){return db
          .prepare(
            "SELECT accepted_user_id FROM beta_team_invitations WHERE id=?",
          )
          .bind(id)
          .first<{ accepted_user_id: string }>();},
guardBranchCapacity(key:string,value:"branch-capacity",companyId:string,companyId2:string){return assertion(
        db,
        key,
        value,
        `${organization.companyBranchCount}<(SELECT branch_limit FROM beta_policy WHERE company_id=?)`,
        [companyId, companyId2],
      );},...owner0(db),...owner1(db),...owner2(db),...owner3(db),async batch(effects){await db.batch(effects);},commit:(...args)=>executeCommand(db,...args),authorityGuard:(...args)=>authorityGuard(db,...args),memberStatements:(...args)=>memberStatements(db,...args),branchStatements:(...args)=>branchStatements(db,...args)};}
function application(db:SqlDatabase){return createBetaTeamApplication({parseGrant:input=>grantInput.parse(input),repository:repository(db),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash:sha256,requireCompanyAuthority:(...args)=>inward(()=>requireCompanyAuthority(db,...args)),requireFreshMfa:(...args)=>inward(()=>requireFreshMfa(...args)),invitationCommand:(actor,key,operation,input,permission,action)=>inward(()=>iamCommand(db as PostgresDatabase,actor,key,operation,input,permission,c=>action(invitation(db,c))))});}
export async function inviteMember(db: SqlDatabase,actor: StaffActor,rawInput: z.input<typeof invitationSchema>,key: string,token: string){return exposed(()=>application(db).inviteMember(actor,rawInput,key,token));}
export async function acceptMember(db: SqlDatabase,user: BetaUser,token: string){db=iamDb(db);return exposed(()=>application(db).acceptMember(user,token));}
export async function addBetaBranch(db: SqlDatabase,actor: StaffActor,name: string,key: string){return exposed(()=>application(db).addBetaBranch(actor,name,key));}

