import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {attachmentOwnerFactReadSql} from "./attachment-owner-fact-read-sql";
export function policyMarketingFacts(db:SqlDatabase){return {readMarketingCampaign:(facts:{targetId:string;companyId:string;})=>{return db.prepare(attachmentOwnerFactReadSql('campaign')).bind(facts.targetId,facts.companyId).first<{ company_id: string; version: number; }>();}};}
