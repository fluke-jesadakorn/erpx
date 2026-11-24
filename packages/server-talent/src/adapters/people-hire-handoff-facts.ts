import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';import type {Source} from '@erpx/server-people_payroll/public/people-handoff-contract';import {assertion} from "@erpx/server-workflow/public/command-assertions";import {ApiError} from "@erpx/server-runtime/public/http";import {nativeTalentReadAll,nativeTalentReadFirst} from "./native-talent-read";import {existsVersionRow} from "./talent-read-tail-r27-sources";
// Wave 3: the four retained write effects below execute only in native owner
// routes (Talent candidate privacy in talent_lifecycle; hire-consumption in the
// People Payroll handoff port). No TypeScript SQL writer remains here; the
// methods stay present and fail closed so cross-owner callers adopt the native
// client rather than silently executing a fallback write.
// The hire-source, hire-queue, hire-consumption receipt and candidate privacy
// row reads below are owned by the native Talent command-plan registry; no TS
// SQL mirror remains for them.
function nativeWriteUnavailable():never{throw new ApiError(503,'NATIVE_TALENT_HANDOFF_UNAVAILABLE','The native Talent handoff command is unavailable.');}
const candidateLockSql=(mode:'SHARE'|'UPDATE')=>`SELECT id FROM talent_candidates WHERE id=? FOR ${mode}`;
export function peopleHireHandoffFacts(database:SqlDatabase){return {readHireSourceRow(id:string,companyId:string){return nativeTalentReadFirst<Source>('talent_rd_hire_source_row',[id,companyId],database);},
async readHireHandoffQueueEffect(companyId:string){const results=await nativeTalentReadAll<Record<string,unknown>>('talent_rd_hire_handoff_queue',[companyId],database);return {results};},
readConsumeHireHandoffReceipt(id:string,companyId:string){return nativeTalentReadFirst<Record<string,unknown>>('talent_rd_consume_hire_handoff_receipt',[id,companyId],database);},
stageConsumeHireHandoffEffect2(candidate_id:string){return database.prepare(candidateLockSql("SHARE")).bind(candidate_id);},
guard9(key:string,value:"hire-source",id:string,companyId:string,expectedOfferVersion:number,expectedCandidateVersion:number,today:string){return assertion(database,key,value,"EXISTS(SELECT 1 FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id JOIN talent_candidates c ON c.id=h.candidate_id WHERE h.id=? AND h.company_id=? AND o.status='accepted' AND o.version=? AND h.source_offer_version=o.version AND c.version=? AND c.status='active' AND c.retention_until>=?)",[id,companyId,expectedOfferVersion,expectedCandidateVersion,today]);},
guard10(key:string,value:"hire-not-consumed",id:string){return assertion(database,key,value,"NOT EXISTS(SELECT 1 FROM talent_hire_consumption_receipts WHERE handoff_id=?)",[id]);},
stageConsumeHireHandoffEffect6(..._args:unknown[]):never{return nativeWriteUnavailable();},
readChangeCandidatePrivacyRow(candidateId:string,companyId:string){return nativeTalentReadFirst<{id:string;status:string;privacy_hold:boolean;privacy_requested_by:string|null;version:number;retention_until:string}>('talent_rd_change_candidate_privacy_row',[candidateId,companyId],database);},
stageChangeCandidatePrivacyEffect(candidateId:string){return database.prepare(candidateLockSql("UPDATE")).bind(candidateId);},
guard21(key:string,value:"candidate-privacy",candidateId:string,companyId:string,version:number,status:string,value2:0 | 1){return assertion(database,key,value,existsVersionRow("talent_candidates"," AND status=? AND privacy_hold::int=?"),[candidateId,companyId,version,status,value2]);},
stageChangeCandidatePrivacyEffect2(..._args:unknown[]):never{return nativeWriteUnavailable();},
stageChangeCandidatePrivacyEffect3(..._args:unknown[]):never{return nativeWriteUnavailable();},
stageChangeCandidatePrivacyEffect4(..._args:unknown[]):never{return nativeWriteUnavailable();}};}
