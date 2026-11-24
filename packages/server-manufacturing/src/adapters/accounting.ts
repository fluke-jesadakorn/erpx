import {manufacturingPostingProfileSchema,manufacturingPostingProfileDecisionSchema} from '@erpx/server-manufacturing/domain/posting-profiles';
import {nativeManufacturingProfile} from '@erpx/server-manufacturing/adapters/native-accounting-profile';
import {ownerWrite,flushOwnerWrites,isOwnerWrite} from '@erpx/server-manufacturing/adapters/native-owner-write';
import {manufacturingAccountingProfileSelect} from './manufacturing-read-tail-r19-sql';
import "server-only";

import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {prepareJournal} from '@erpx/server-finance/public/journal-planning';

import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import {BusinessRuleError} from '@erpx/server-shared/index';import {createManufacturingAccountingApplication,type ManufacturingAccountingRepository} from '@erpx/server-manufacturing/application/accounting';import {manufacturingAccountingSources as owner0} from '@erpx/server-commerce/public/manufacturing-accounting-sources';
import {manufacturingAccountingSources as owner1} from '@erpx/server-finance/public/manufacturing-accounting-sources';async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
// Accounting-profile create/review is native-only (accounting_profile_wave3);
// no TypeScript writer, fallback or profile read remains for that capability.
function application(db:SqlDatabase,boundRequest?:unknown){const repository:ManufacturingAccountingRepository<SqlStatement>={async approvedProfile(companyId:string){return db.prepare(manufacturingAccountingProfileSelect("*","","company_id=? AND status='approved'"))
    .bind(companyId).first<Row>();},
guardApprovedProfile(key:string,value:"production-accounting-profile",value2:string,companyId:string,value3:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM manufacturing_accounting_profiles WHERE id=? AND company_id=? AND status='approved' AND version=?)", [value2, companyId, value3]);},
preparePostingEvidence(value:string,companyId:string,orderId:string,orderVersion:number,phase:"output" | "issue" | "report" | "scrap",id:string | number | null,version:string | number | null,id2:string,total:number,value2:string,userId:string,now:number){return ownerWrite('production.posting',[value,companyId,orderId,orderVersion,phase,id,version,id2,total,value2,userId,now],String(value));},
prepareCostMovement(id:string,companyId:string,orderId:string,kind:string,value:number,amount:number,sourceId:string,id2:string,entryDate:string,userId:string,now:number){return ownerWrite('production.cost-movement',[id,companyId,orderId,kind,value,amount,sourceId,id2,entryDate,userId,now],String(id));},...owner0(db),...owner1(db)};return createManufacturingAccountingApplication({repository,clock:{now:()=>Date.now()},requirePermission:(...args)=>{try{return requirePermission(...args);}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}},commands:{replay:(...args)=>{if(boundRequest!==undefined)args[4]=boundRequest;return inward(()=>replayCommand(db,...args));},commit:async(...args)=>{if(boundRequest!==undefined)args[3]=boundRequest;const a=args as unknown[],b=(a[9] as object|undefined)??{},actor=args[0] as WorkContext,key=String(args[1]);a[9]={...b,companyId:actor.companyId,native:(effect:SqlStatement)=>isOwnerWrite(effect),flush:(effects:SqlStatement[])=>flushOwnerWrites(effects,actor,key)};return inward(()=>executeCommand(db,...args));}},prepareJournal:(...args)=>inward(()=>prepareJournal(db,...args)),snapshotMoney:(...args)=>inward(()=>snapshotMoney(db,...args))});}
export async function prepareManufacturingPosting(db: SqlDatabase,a: WorkContext,input: {
  key: string; orderId: string; orderVersion: number; phase: "issue" | "report" | "output" | "scrap";
  entryDate: string; material?: number; labor?: number; overhead?: number; output?: number; quantityMilli?: number;
}){return exposed(()=>application(db).prepareManufacturingPosting(a,input));}

export async function prepareManufacturingPostingProfile(actor:WorkContext,raw:unknown,key:string){const request=manufacturingPostingProfileSchema.parse(raw);return nativeManufacturingProfile('create',null,request.input,key,actor);}
export async function decideManufacturingPostingProfile(actor:WorkContext,id:string,raw:unknown,key:string){const request=manufacturingPostingProfileDecisionSchema.parse(raw);return nativeManufacturingProfile('review',id,request.input,key,actor);}
