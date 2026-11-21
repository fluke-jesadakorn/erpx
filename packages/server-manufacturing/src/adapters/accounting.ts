import {manufacturingPostingProfileSchema,manufacturingPostingProfileDecisionSchema} from '@erpx/server-manufacturing/domain/posting-profiles';
import {withManufacturingPostingProfileCommand} from '@erpx/server-manufacturing/adapters/posting-profiles';
import {ownerWrite,flushOwnerWrites} from '@erpx/server-manufacturing/adapters/native-owner-write';
import "server-only";

import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {prepareJournal} from '@erpx/server-finance/public/journal-planning';

import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import {BusinessRuleError} from '@erpx/server-shared/index';import {createManufacturingAccountingApplication,type ManufacturingAccountingRepository} from '@erpx/server-manufacturing/application/accounting';import {manufacturingAccountingSources as owner0} from '@erpx/server-commerce/public/manufacturing-accounting-sources';
import {manufacturingAccountingSources as owner1} from '@erpx/server-finance/public/manufacturing-accounting-sources';async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
// Wave 3: profile writes execute only in the native axum route
// (accounting_profile_wave3). The retained repository methods are fail-closed:
// no SQL and no TypeScript fallback remains for this capability.
function nativeProfileUnavailable():never{throw new ApiError(503,'NATIVE_MANUFACTURING_UNAVAILABLE','The native Manufacturing profile command is unavailable.');}
function application(db:SqlDatabase,boundRequest?:unknown){const repository:ManufacturingAccountingRepository<SqlStatement>={prepareProfile():SqlStatement{return nativeProfileUnavailable();},
async profile(id:string,companyId:string){return db.prepare("SELECT * FROM manufacturing_accounting_profiles WHERE id=? AND company_id=?")
    .bind(id, companyId).first<Row>();},
guardProfileVersion(key:string,value:"profile",id:string,companyId:string,expectedVersion:number,expected:"approved" | "draft",userId:string){return assertion(db, key, value, "EXISTS(SELECT 1 FROM manufacturing_accounting_profiles WHERE id=? AND company_id=? AND version=? AND status=? AND created_by<>?)", [id, companyId, expectedVersion, expected, userId]);},
prepareProfileReview():SqlStatement{return nativeProfileUnavailable();},
async approvedProfile(companyId:string){return db.prepare("SELECT * FROM manufacturing_accounting_profiles WHERE company_id=? AND status='approved'")
    .bind(companyId).first<Row>();},
guardApprovedProfile(key:string,value:"production-accounting-profile",value2:string,companyId:string,value3:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM manufacturing_accounting_profiles WHERE id=? AND company_id=? AND status='approved' AND version=?)", [value2, companyId, value3]);},
preparePostingEvidence(value:string,companyId:string,orderId:string,orderVersion:number,phase:"output" | "issue" | "report" | "scrap",id:string | number | null,version:string | number | null,id2:string,total:number,value2:string,userId:string,now:number){return ownerWrite('production.posting',[value,companyId,orderId,orderVersion,phase,id,version,id2,total,value2,userId,now],String(value));},
prepareCostMovement(id:string,companyId:string,orderId:string,kind:string,value:number,amount:number,sourceId:string,id2:string,entryDate:string,userId:string,now:number){return ownerWrite('production.cost-movement',[id,companyId,orderId,kind,value,amount,sourceId,id2,entryDate,userId,now],String(id));},...owner0(db),...owner1(db)};return createManufacturingAccountingApplication({repository,clock:{now:()=>Date.now()},requirePermission:(...args)=>{try{return requirePermission(...args);}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}},commands:{replay:(...args)=>{if(boundRequest!==undefined)args[4]=boundRequest;return inward(()=>replayCommand(db,...args));},commit:async(...args)=>{if(boundRequest!==undefined)args[3]=boundRequest;const effects=await flushOwnerWrites((args[7] as unknown[])??[],args[0] as WorkContext,String(args[1]));args[7]=effects;return inward(()=>executeCommand(db,...args));}},prepareJournal:(...args)=>inward(()=>prepareJournal(db,...args)),snapshotMoney:(...args)=>inward(()=>snapshotMoney(db,...args))});}export async function createManufacturingAccountingProfile(a: WorkContext,raw: unknown,key: string){const {write:db}=await holdingResources(a.holdingId);const i=profileSchema.parse(raw);return withManufacturingPurchasedCommand(db,a,{key,operation:"manufacturing.accounting.create",target:{kind:"accounting",id:`mfg-accounting-${key}`},purpose:"new-work",permission:"finance.write"},tx=>exposed(()=>application(tx).createManufacturingAccountingProfile(a,i,key)));}
export async function reviewManufacturingAccountingProfile(a: WorkContext,id: string,raw: unknown,key: string){const {write:db}=await holdingResources(a.holdingId);const i=reviewSchema.parse(raw);return withManufacturingPurchasedCommand(db,a,{key,operation:`manufacturing.accounting.${i.action}`,target:{kind:"accounting",id},purpose:i.action==="approve"?"new-work":"settle-existing",permission:"finance.approve",existing:true},tx=>exposed(()=>application(tx).reviewManufacturingAccountingProfile(a,id,i,key)));}
export async function prepareManufacturingPosting(db: SqlDatabase,a: WorkContext,input: {
  key: string; orderId: string; orderVersion: number; phase: "issue" | "report" | "output" | "scrap";
  entryDate: string; material?: number; labor?: number; overhead?: number; output?: number; quantityMilli?: number;
}){return exposed(()=>application(db).prepareManufacturingPosting(a,input));}

import {profileSchema,reviewSchema} from "@erpx/server-manufacturing/domain/accounting";
import {withManufacturingPurchasedCommand} from "@erpx/server-manufacturing/public/purchases";

export async function prepareManufacturingPostingProfile(actor:WorkContext,raw:unknown,key:string){const request=manufacturingPostingProfileSchema.parse(raw);return withManufacturingPostingProfileCommand(actor,null,request,key,tx=>exposed(()=>application(tx,request).createManufacturingAccountingProfile(actor,request.input,key)));}
export async function decideManufacturingPostingProfile(actor:WorkContext,id:string,raw:unknown,key:string){const request=manufacturingPostingProfileDecisionSchema.parse(raw);return withManufacturingPostingProfileCommand(actor,id,request,key,tx=>exposed(()=>application(tx,request).reviewManufacturingAccountingProfile(actor,id,request.input,key)));}
