import {clearAgentBootstrapAssertion} from "../../workflow/public/agent-bootstrap-assertions";
import {onboardingSlugBinding} from '../../runtime/public/identity-management-binding-facts';
import {betaManagementOrganizationSources} from '../../organization/public/beta-management-sources';
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
import { z } from "zod";
import {
  resourceSlots,
  registeredResources,
  type RegistryEnv,
} from "@/lib/holding-registry";
import { betaLimits, type BetaEnvironment } from "@/server/beta-config";
import { ApiError } from "@/server/modules/runtime/public/http";


import type {SqlStatement} from '../../runtime/public/runtime/types';
import {BusinessRuleError} from '../../shared';
import {createBetaOnboardingApplication,createBetaBootstrapEffects,type BetaControlRepository,type BetaHoldingRepository} from '../application/onboarding';
import {onboardingSchema,requireVerified as verified,type BetaUser,type SlotRow} from '../domain/onboarding';
export {onboardingSchema} from '../domain/onboarding';
export type {BetaUser} from '../domain/onboarding';
import {betaOnboardingEffects as owner0} from '../../people_payroll/public/beta-onboarding-effects';
import {betaOnboardingEffects as owner1} from '../../identity_access/public/beta-onboarding-effects';
import {betaOnboardingEffects as owner2} from '../../organization/public/beta-onboarding-effects';
import {betaOnboardingEffects as owner3} from '../../runtime/public/beta-onboarding-effects';
import {betaOnboardingEffects as owner4} from '../../billing/public/beta-onboarding-effects';
type Env=RegistryEnv&BetaEnvironment&{CONTROL_DB:SqlDatabase};
function bindHolding(db:SqlDatabase):BetaHoldingRepository<SqlStatement>{const organization=betaManagementOrganizationSources(db);return {prepareMembership(id:string,invitation:string | null,now:number){return db
      .prepare(
        "INSERT INTO beta_members(user_id,invitation_id,joined_at) VALUES(?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(id, invitation, now);},
async provisioningReceipt(company:string,holding_id:string){return db
    .prepare("SELECT status FROM company_provisioning WHERE company_id=? AND holding_id=?")
    .bind(company, holding_id)
    .first<{ status: string }>();},
guardBootstrapOwner(bootstrapKey:string,holding_id:string,id:string,holding_id2:string,id2:string){return assertion(
        db,
        bootstrapKey,
        "bootstrap-owner",
        `NOT EXISTS(SELECT 1 FROM beta_bootstrap WHERE holding_id<>? OR owner_user_id<>?) AND (EXISTS(SELECT 1 FROM beta_bootstrap WHERE holding_id=? AND owner_user_id=?) OR ${organization.emptyBootstrapForest})`,
        [holding_id, id, holding_id2, id2],
      );},
prepareProvisioning(company:string,holding_id:string,id:string,now:number,now2:number){return db
        .prepare(
          "INSERT INTO company_provisioning(company_id,holding_id,owner_user_id,status,version,requested_at,updated_at) VALUES(?,?,?,'provisioning',1,?,?) ON CONFLICT(company_id) DO NOTHING",
        )
        .bind(company, holding_id, id, now, now2);},
prepareStartedEvent(value:string,company:string,holding_id:string,id:string,value2:string,now:number){return db
        .prepare(
          "INSERT INTO company_provisioning_events(id,company_id,holding_id,action,version,actor_id,detail_json,created_at) VALUES(?,?,?,'started',1,?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(
          value,
          company,
          holding_id,
          id,
          value2,
          now,
        );},
prepareBootstrap(holding_id:string,id:string,now:number){return db
        .prepare(
          "INSERT INTO beta_bootstrap(singleton,holding_id,owner_user_id,created_at) VALUES(1,?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(holding_id, id, now);},
preparePolicy(company:string,members:number,branches:number,ai:number,storage:number,now:number){return db
        .prepare(
          "INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES(?,?,?,?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(
          company,
          members,
          branches,
          ai,
          storage,
          now,
        );},
prepareProvisioned(value:number,value2:number,company:string,holding_id:string){return db
        .prepare(
          "UPDATE company_provisioning SET status='active',version=2,completed_at=?,updated_at=?,last_error=NULL WHERE company_id=? AND holding_id=?",
        )
        .bind(value, value2, company, holding_id);},
prepareCompletedEvent(value:string,company:string,holding_id:string,id:string,value2:string,value3:number){return db
        .prepare(
          "INSERT INTO company_provisioning_events(id,company_id,holding_id,action,version,actor_id,detail_json,created_at) VALUES(?,?,?,'completed',2,?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(
          value,
          company,
          holding_id,
          id,
          value2,
          value3,
        );},
clearBootstrapAssertions(bootstrapKey:string){return clearAgentBootstrapAssertion(db,bootstrapKey);},
async completedReceipt(company:string,holding_id:string){return db
    .prepare("SELECT status FROM company_provisioning WHERE company_id=? AND holding_id=? AND status='active'")
    .bind(company, holding_id)
    .first<{ status: string }>();},...owner0(db),...owner1(db),...owner2(db),...owner3(db),...owner4(db),async batch(effects){await db.batch(effects);}};}
function bindControl(db:SqlDatabase):BetaControlRepository<SqlStatement>{return {onboardingSlot:userId=>db.prepare('SELECT slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=?').bind(userId).first<{slug:string;company_name:string;branch_name:string;state:string}>(),async ownerSlot(id:string){return db
    .prepare("SELECT * FROM beta_slots WHERE owner_user_id=?")
    .bind(id)
    .first<SlotRow>();},
async slugBinding(slug:string){return onboardingSlugBinding(db,slug);},
async claimSlot(id:string,slug:string,companyName:string,branchName:string,now:number,id2:string){return db
        .prepare(
          `UPDATE beta_slots SET owner_user_id=?,slug=?,company_name=?,branch_name=?,state='provisioning',updated_at=? WHERE holding_id=(SELECT holding_id FROM beta_slots WHERE state='available' ORDER BY holding_id FOR UPDATE SKIP LOCKED LIMIT 1) AND state='available' AND NOT EXISTS(SELECT 1 FROM beta_slots WHERE owner_user_id=?) RETURNING *`,
        )
        .bind(
          id,
          slug,
          companyName,
          branchName,
          now,
          id2,
        )
        .first<SlotRow>();},
async recoverOwnerSlot(id:string){return db
        .prepare("SELECT * FROM beta_slots WHERE owner_user_id=?")
        .bind(id)
        .first<SlotRow>();},
async currentOwnerSlot(id:string){return db
        .prepare("SELECT * FROM beta_slots WHERE owner_user_id=?")
        .bind(id)
        .first<SlotRow>();},
async waitlist(id:string,now:number){await db
        .prepare(
          "INSERT INTO beta_waitlist(user_id,created_at) VALUES(?,?) ON CONFLICT DO NOTHING",
        )
        .bind(id, now)
        .run();},
prepareActiveSlot(now:number,holding_id:string,id:string){return db
      .prepare(
        "UPDATE beta_slots SET state='active',updated_at=? WHERE holding_id=? AND owner_user_id=?",
      )
      .bind(now, holding_id, id);},...owner0(db),...owner1(db),...owner2(db),...owner3(db),...owner4(db),async batch(effects){await db.batch(effects);}};}
function exposedSync<T>(action:()=>T){try{return action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function requireVerified(user:BetaUser|undefined):asserts user is BetaUser{exposedSync(()=>verified(user));}
export function memberStatements(db:SqlDatabase,company:string,branch:string,user:BetaUser,owner:boolean,invitation:string|null,grant?:{parentId:string;validUntil:number}){return createBetaBootstrapEffects(bindHolding(db),{now:()=>Date.now()}).memberStatements(company,branch,user,owner,invitation,grant);}
export function branchStatements(db:SqlDatabase,company:string,id:string,name:string,slug:string){return createBetaBootstrapEffects(bindHolding(db),{now:()=>Date.now()}).branchStatements(company,id,name,slug);}
export async function onboard(env:Env,user:BetaUser,input:z.infer<typeof onboardingSchema>){try{return await createBetaOnboardingApplication({control:bindControl(env.CONTROL_DB),holding:id=>bindHolding(registeredResources(env,id).write),publicBeta:env.ERPX_PUBLIC_BETA==='true',resourceSlots:()=>resourceSlots(env),limits:()=>betaLimits(env),clock:{now:()=>Date.now()}}).onboard(user,input);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}

export async function onboardingView(env:Env,user:BetaUser){try{return await createBetaOnboardingApplication({control:bindControl(env.CONTROL_DB),holding:id=>bindHolding(registeredResources(env,id).write),publicBeta:env.ERPX_PUBLIC_BETA==='true',resourceSlots:()=>resourceSlots(env),limits:()=>betaLimits(env),clock:{now:()=>Date.now()}}).view(user);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
