import "server-only";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {legalCorpusSourceFacts} from "@erpx/server-files/public/legal-corpus-source-facts";
import {access,guards} from "@erpx/server-legal/adapters/legal/access";
import {documentAccess} from "@erpx/server-legal/adapters/legal/documents";
import {hash} from "@erpx/server-legal/adapters/legal/service";
import {createLegalLawCorpusApplication} from "@erpx/server-legal/application/law-corpus";
import {officialLawUrl as canonicalUrl,bangkokSourceCheckDate as checkDate,type ReviewRow} from "@erpx/server-legal/domain/law-corpus";
import type {LegalWrite} from "@erpx/server-legal/application/service";
import { legalLawCorpus } from '@erpx/server-collaboration/adapters/native-legal-law-corpus';
import { legalAttachmentAvailableSql } from '../legal-read-tail-r47-sources';
import { legalDisposedGuardSql } from '../legal-read-tail-r39-sources';
import { legalCorpusSourceCleanHead } from '../legal-read-tail-r48-sources';
import { legalLawStateGuardSql } from '../legal-read-tail-r51-sources';
import { legalPointByIdKeyAndCompanySql } from '../legal-read-tail-r53-sources';
import { legalStatePointSql } from '../legal-read-tail-r33-sources';
import { legalPointByIdAndKeySql } from '../legal-read-tail-r34-sources';
import { legalLawSourceCheckPendingSql, legalLawSourceCurrentSql, legalLawSourceCheckCurrentSql, legalLawApplicabilityCurrentSql, legalLawReviewCurrentSql } from '../legal-read-tail-r56-sources';
import {nativeLegalAuxiliaryRead} from '../native-legal-auxiliary';
export {legalChunkSchema} from "@erpx/server-legal/domain/law-corpus";
export {fetchOfficialLawSource} from "@erpx/server-legal/adapters/legal/law-source-provider";
export {drainLegalSourceChecks} from "@erpx/server-legal/adapters/legal/law-source-worker";
export function officialLawUrl(value:string){return canonicalUrl(value,(status,code,message)=>new ApiError(status,code,message));}
export function bangkokSourceCheckDate(now=Date.now()){return checkDate(now);}
const reviewTables={law:{table:"legal_law_revisions",column:"rule_id"},document:{table:"legal_text_reviews",column:"item_id"}} as const;
/** Exact projection of the native `laws` auxiliary read; no view SQL remains in TypeScript. */
type LawCorpusView={
  sources:Record<string,unknown>[];
  revisions:Record<string,unknown>[];
  applicability:Record<string,unknown>[];
  textReviews:Record<string,unknown>[];
  checks:Record<string,unknown>[];
  chunks:({item_id:string;document_id:string}&Record<string,unknown>)[];
  impacts:Record<string,unknown>[];
  notice:string;
};
function application(db:SqlDatabase,a:WorkContext,view?:LawCorpusView){const nativeCorpus=legalLawCorpus(db,a.companyId,a.userId);const fileFacts=legalCorpusSourceFacts(db),nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};
return createLegalLawCorpusApplication({actor:a,access:(id,p)=>access(db,a,id,p),document:id=>documentAccess(db,a,id),require:p=>requirePermission(a,p),samePerson:id=>samePerson(a,id),hash,clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
cleanSourceLink:facts=>fileFacts.cleanSourceLink(facts),
inspection:(facts:{inspectionId:string;documentId:string;})=>db.prepare(legalPointByIdAndKeySql('result_json','legal_document_inspections','document_id')+" AND kind='ocr'").bind(facts.inspectionId,facts.documentId).first<{ result_json: string }>(),
async insertChunk(facts){return stage(await nativeCorpus.insertChunk(facts));},
async registerSource(facts){return stage(await nativeCorpus.registerSource(facts));},
registeredSource:(facts:{sourceId:string;id:string;companyId:string;})=>db.prepare(legalLawStateGuardSql('legal_law_sources','active')).bind(facts.sourceId,facts.id,facts.companyId).first(),
pendingCheck:(facts:{sourceCheckId:string;sourceId:string;companyId:string;checksum:string;})=>db.prepare(legalLawSourceCheckPendingSql()).bind(facts.sourceCheckId,facts.sourceId,facts.companyId,facts.checksum).first(),
async importRevision(facts){return stage(await nativeCorpus.importRevision(facts));},
async bindSourceCheck(facts){return stage(await nativeCorpus.bindSourceCheck(facts));},
async importDocumentText(facts){return stage(await nativeCorpus.importDocumentText(facts));},
approvedRevision:(facts:{revisionId:string;id:string;companyId:string;})=>db.prepare(legalLawStateGuardSql('legal_law_revisions','approved')).bind(facts.revisionId,facts.id,facts.companyId).first(),
async proposeApplicability(facts){return stage(await nativeCorpus.proposeApplicability(facts));},
applicability:(facts:{applicabilityId:string;id:string;companyId:string;})=>db.prepare(legalPointByIdKeyAndCompanySql('id,version,state,created_by','legal_law_applicability','rule_id')).bind(facts.applicabilityId,facts.id,facts.companyId).first<ReviewRow>(),
async approveApplicability(facts){return stage(await nativeCorpus.approveApplicability(facts));},
review:(facts:{id:string;id2:string;companyId:string;reviewKind:"law"|"document";})=>db.prepare(legalPointByIdKeyAndCompanySql('*',reviewTables[facts.reviewKind].table,reviewTables[facts.reviewKind].column)).bind(facts.id,facts.id2,facts.companyId).first<ReviewRow>(),
decideReview:(facts:{fact0:"approved" | "retired";actorId:string;now:number;reason:string;id:string;reviewKind:"law"|"document";})=>stage(db.prepare(`UPDATE ${reviewTables[facts.reviewKind].table} SET state=?,reviewed_by=?,reviewed_at=?,review_reason=?,version=version+1 WHERE id=?`).bind(facts.fact0,facts.actorId,facts.now,facts.reason,facts.id)),
async reviewSourceCheck(facts){return stage(await nativeCorpus.reviewSourceCheck(facts));},
async advanceRecord(facts){return stage(await nativeCorpus.advanceRecord({now:facts.now,id:facts.id}));},
requireLegalCorpusSourceClean:(key:string,values:string[])=>stage(assertion(db,key,"legal-corpus-source-clean",`EXISTS(${legalCorpusSourceCleanHead(`d.item_id=? AND i.company_id=? AND d.checksum=? AND ${legalDisposedGuardSql('x','d.id')} AND ${legalAttachmentAvailableSql()}`)})`,values)),
requireLawSourceCurrent:(key:string,values:string[])=>stage(assertion(db,key,"law-source-current",`EXISTS(${legalLawSourceCurrentSql()})`,values)),
requireLawSourceCheckCurrent:(key:string,values:string[])=>stage(assertion(db,key,"law-source-check-current",`EXISTS(${legalLawSourceCheckCurrentSql()})`,values)),
requireLawRevisionCurrent:(key:string,values:string[])=>stage(assertion(db,key,"law-revision-current",`EXISTS(${legalStatePointSql('1','legal_law_revisions','rule_id','approved')})`,values)),
requireLawApplicabilityCurrent:(key:string,values:(string | number)[])=>stage(assertion(db,key,"law-applicability-current",`EXISTS(${legalLawApplicabilityCurrentSql()})`,values)),
requireLawReviewCurrent:(key:string,reviewKind:"law"|"document",retiring:boolean,values:(string | number)[])=>stage(assertion(db,key,"law-review-current",`EXISTS(${legalLawReviewCurrentSql(reviewTables[reviewKind].table,retiring)})`,values)),
sources:async()=>view?.sources??[],revisions:async()=>view?.revisions??[],applicabilityList:async()=>view?.applicability??[],textReviews:async()=>view?.textReviews??[],checks:async()=>view?.checks??[],impacts:async()=>view?.impacts??[],chunks:async()=>view?.chunks??[],recordReads:async()=>{}, // native `laws` read owns corpus access events
guardChanges:(id,version,key,edit,allowFrozen,preparation)=>guards(db,a,id,version,key,edit,allowFrozen,preparation).map(stage),event:async(id,action,note,version,identifier,now)=>stage(await nativeCorpus.event({id,action,note,version,identifier,now})),replay:<Result extends object=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown)=>replayCommand<Result>(db,a,key,operation,id,input),commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_CORPUS_REPOSITORY_MISMATCH");return statement;})),
}});}
export function lawCorpusCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).lawCorpusCommand(raw,key);}
export async function lawCorpusView(db:SqlDatabase,a:WorkContext,ruleId?:string){requirePermission(a,"legal.read");const view=await nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId);return application(db,a,view).lawCorpusView(ruleId);}
