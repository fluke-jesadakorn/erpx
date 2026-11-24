import "server-only";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {legalCorpusSourceFacts} from "@erpx/server-files/public/legal-corpus-source-facts";
import {access,guards,visibility} from "@erpx/server-legal/adapters/legal/access";
import {documentAccess} from "@erpx/server-legal/adapters/legal/documents";
import {hash} from "@erpx/server-legal/adapters/legal/service";
import {createLegalLawCorpusApplication} from "@erpx/server-legal/application/law-corpus";
import {officialLawUrl as canonicalUrl,bangkokSourceCheckDate as checkDate,type ReviewRow} from "@erpx/server-legal/domain/law-corpus";
import type {LegalWrite} from "@erpx/server-legal/application/service";
import { legalLawCorpus } from '@erpx/server-collaboration/adapters/native-legal-law-corpus';
import { legalActiveDestructionGuardHead } from '../legal-read-tail-r46-sources';
import { legalAttachmentAvailableSql } from '../legal-read-tail-r47-sources';
import { legalDisposedGuardSql } from '../legal-read-tail-r39-sources';
export {legalChunkSchema} from "@erpx/server-legal/domain/law-corpus";
export {fetchOfficialLawSource} from "@erpx/server-legal/adapters/legal/law-source-provider";
export {drainLegalSourceChecks} from "@erpx/server-legal/adapters/legal/law-source-worker";
export function officialLawUrl(value:string){return canonicalUrl(value,(status,code,message)=>new ApiError(status,code,message));}
export function bangkokSourceCheckDate(now=Date.now()){return checkDate(now);}
const reviewTables={law:{table:"legal_law_revisions",column:"rule_id"},document:{table:"legal_text_reviews",column:"item_id"}};
function application(db:SqlDatabase,a:WorkContext){const nativeCorpus=legalLawCorpus(db,a.companyId,a.userId);const fileFacts=legalCorpusSourceFacts(db),nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};function viewScope(ruleId?:string){
  const v = visibility(a), clause = `${v.sql}${ruleId ? " AND i.id=?" : ""}`, args = [...v.values, ...(ruleId ? [ruleId] : [])];
  const select = async (table: string, itemColumn: string, fields = "c.*") => (await db.prepare(`SELECT ${fields},i.title record_title FROM ${table} c JOIN legal_items i ON i.id=c.${itemColumn} WHERE ${clause} ORDER BY c.created_at DESC,c.id DESC LIMIT 200`).bind(...args).all()).results;
  const ruleVisibility = visibility(a, "rule");

return {select,impacts:async()=>(await db.prepare(`SELECT c.id check_id,i.id item_id,i.title,i.state,i.due_date,i.version FROM legal_law_source_checks c JOIN legal_items rule ON rule.id=c.rule_id JOIN legal_links l ON l.source_id=rule.id JOIN legal_items i ON i.id=l.target_id WHERE c.state='pending_review' AND ${ruleVisibility.sql} AND ${v.sql}${ruleId ? " AND rule.id=?" : ""} ORDER BY c.created_at DESC,i.id LIMIT 200`).bind(...ruleVisibility.values, ...args).all()).results,chunks:async()=>(await db.prepare(`SELECT c.*,i.title record_title FROM legal_source_chunks c JOIN legal_documents d ON d.id=c.document_id JOIN legal_items i ON i.id=c.item_id WHERE ${clause} AND ${legalDisposedGuardSql('x','c.document_id')} ${legalActiveDestructionGuardHead('x','i')} AND ${legalAttachmentAvailableSql()} ORDER BY c.document_id,c.page,c.id LIMIT 1000`).bind(...args).all<{ item_id: string; document_id: string } & Record<string, unknown>>()).results};}
return createLegalLawCorpusApplication({actor:a,access:(id,p)=>access(db,a,id,p),document:id=>documentAccess(db,a,id),require:p=>requirePermission(a,p),samePerson:id=>samePerson(a,id),hash,clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
cleanSourceLink:facts=>fileFacts.cleanSourceLink(facts),
inspection:(facts:{inspectionId:string;documentId:string;})=>db.prepare("SELECT result_json FROM legal_document_inspections WHERE id=? AND document_id=? AND kind='ocr'").bind(facts.inspectionId,facts.documentId).first<{ result_json: string }>(),
async insertChunk(facts){return stage(await nativeCorpus.insertChunk(facts));},
async registerSource(facts){return stage(await nativeCorpus.registerSource(facts));},
registeredSource:(facts:{sourceId:string;id:string;companyId:string;})=>db.prepare("SELECT 1 FROM legal_law_sources WHERE id=? AND rule_id=? AND company_id=? AND state='active'").bind(facts.sourceId,facts.id,facts.companyId).first(),
pendingCheck:(facts:{sourceCheckId:string;sourceId:string;companyId:string;checksum:string;})=>db.prepare("SELECT 1 FROM legal_law_source_checks WHERE id=? AND source_id=? AND company_id=? AND state='pending_review' AND checksum=? AND candidate_revision_id IS NULL").bind(facts.sourceCheckId,facts.sourceId,facts.companyId,facts.checksum).first(),
async importRevision(facts){return stage(await nativeCorpus.importRevision(facts));},
async bindSourceCheck(facts){return stage(await nativeCorpus.bindSourceCheck(facts));},
async importDocumentText(facts){return stage(await nativeCorpus.importDocumentText(facts));},
approvedRevision:(facts:{revisionId:string;id:string;companyId:string;})=>db.prepare("SELECT 1 FROM legal_law_revisions WHERE id=? AND rule_id=? AND company_id=? AND state='approved'").bind(facts.revisionId,facts.id,facts.companyId).first(),
async proposeApplicability(facts){return stage(await nativeCorpus.proposeApplicability(facts));},
applicability:(facts:{applicabilityId:string;id:string;companyId:string;})=>db.prepare("SELECT id,version,state,created_by FROM legal_law_applicability WHERE id=? AND rule_id=? AND company_id=?").bind(facts.applicabilityId,facts.id,facts.companyId).first<ReviewRow>(),
async approveApplicability(facts){return stage(await nativeCorpus.approveApplicability(facts));},
review:(facts:{id:string;id2:string;companyId:string;reviewKind:"law"|"document";})=>db.prepare(`SELECT * FROM ${reviewTables[facts.reviewKind].table} WHERE id=? AND ${reviewTables[facts.reviewKind].column}=? AND company_id=?`).bind(facts.id,facts.id2,facts.companyId).first<ReviewRow>(),
decideReview:(facts:{fact0:"approved" | "retired";actorId:string;now:number;reason:string;id:string;reviewKind:"law"|"document";})=>stage(db.prepare(`UPDATE ${reviewTables[facts.reviewKind].table} SET state=?,reviewed_by=?,reviewed_at=?,review_reason=?,version=version+1 WHERE id=?`).bind(facts.fact0,facts.actorId,facts.now,facts.reason,facts.id)),
async reviewSourceCheck(facts){return stage(await nativeCorpus.reviewSourceCheck(facts));},
async advanceRecord(facts){return stage(await nativeCorpus.advanceRecord({now:facts.now,id:facts.id}));},
requireLegalCorpusSourceClean:(key:string,values:string[])=>stage(assertion(db,key,"legal-corpus-source-clean",`EXISTS(SELECT 1 FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND d.item_id=? AND i.company_id=? AND d.checksum=? AND ${legalDisposedGuardSql('x','d.id')} AND ${legalAttachmentAvailableSql()})`,values)),
requireLawSourceCurrent:(key:string,values:string[])=>stage(assertion(db,key,"law-source-current","EXISTS(SELECT 1 FROM legal_law_sources WHERE id=? AND company_id=? AND rule_id=? AND state='active')",values)),
requireLawSourceCheckCurrent:(key:string,values:string[])=>stage(assertion(db,key,"law-source-check-current","EXISTS(SELECT 1 FROM legal_law_source_checks WHERE id=? AND state='pending_review' AND checksum=? AND candidate_revision_id IS NULL)",values)),
requireLawRevisionCurrent:(key:string,values:string[])=>stage(assertion(db,key,"law-revision-current","EXISTS(SELECT 1 FROM legal_law_revisions WHERE id=? AND rule_id=? AND state='approved')",values)),
requireLawApplicabilityCurrent:(key:string,values:(string | number)[])=>stage(assertion(db,key,"law-applicability-current","EXISTS(SELECT 1 FROM legal_law_applicability WHERE id=? AND version=? AND state='pending' AND NOT iam_same_person(created_by,?,company_id))",values)),
requireLawReviewCurrent:(key:string,reviewKind:"law"|"document",retiring:boolean,values:(string | number)[])=>stage(assertion(db,key,"law-review-current",`EXISTS(SELECT 1 FROM ${reviewTables[reviewKind].table} WHERE id=? AND version=? AND state=? ${retiring ? "" : "AND NOT iam_same_person(created_by,?,company_id)"})`,values)),
sources:id=>viewScope(id).select("legal_law_sources","rule_id"),revisions:id=>viewScope(id).select("legal_law_revisions","rule_id"),applicabilityList:id=>viewScope(id).select("legal_law_applicability","rule_id"),textReviews:id=>viewScope(id).select("legal_text_reviews","item_id"),checks:id=>viewScope(id).select("legal_law_source_checks","rule_id","c.id,c.source_id,c.company_id,c.rule_id,c.check_date,c.state,c.attempts,c.checked_at,c.checksum,c.error,c.candidate_revision_id,c.created_at"),impacts:id=>viewScope(id).impacts(),chunks:id=>viewScope(id).chunks(),recordReads:async reads=>{if(reads.length){const statements=await Promise.all(reads.map(row=>nativeCorpus.accessEvent({identifier:row.identifier,itemId:row.itemId,documentId:row.documentId,action:"corpus.review.read",now:row.now})));await db.batch(statements);}},
guardChanges:(id,version,key,edit,allowFrozen,preparation)=>guards(db,a,id,version,key,edit,allowFrozen,preparation).map(stage),event:async(id,action,note,version,identifier,now)=>stage(await nativeCorpus.event({id,action,note,version,identifier,now})),replay:<Result extends object=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown)=>replayCommand<Result>(db,a,key,operation,id,input),commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_CORPUS_REPOSITORY_MISMATCH");return statement;})),
}});}
export function lawCorpusCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).lawCorpusCommand(raw,key);}
export function lawCorpusView(db:SqlDatabase,a:WorkContext,ruleId?:string){return application(db,a).lawCorpusView(ruleId);}
