import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {qualityAttachmentScopeReadSql} from "./quality-attachment-scope-read-sql";
const descriptorTargets=new Set(["quality_hold","inventory_quality_hold"]);
export function policyQualityFacts(db:SqlDatabase){return {readDescriptor:(facts:{targetId:string;companyId:string;targetType:string;})=>{if(!descriptorTargets.has(facts.targetType))return Promise.resolve(null);return db.prepare(qualityAttachmentScopeReadSql('hold')).bind(facts.targetId,facts.companyId).first<{ company_id: string; branch_id: string | null; department_id: string | null; version: number; }>();},
readQualityHold:(facts:{targetId:string;companyId:string;})=>{return db.prepare(qualityAttachmentScopeReadSql('hold')).bind(facts.targetId,facts.companyId).first<{ company_id: string; branch_id: string | null; department_id: null; version: number; }>();}};}
