import {legalReadScope} from '@erpx/server-legal/adapters/purchases';
import { finalRootOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/final-root-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {access,visibility} from "@erpx/server-legal/adapters/legal/access";
import {documentAccess} from "@erpx/server-legal/public/legal/documents";
import {createLegalKnowledgeApplication} from "@erpx/server-legal/application/knowledge";
import {validateLegalAiCitations as validateCitations,type LegalEvidenceSource,type LegalAiEvidence} from "@erpx/server-legal/domain/knowledge";
import { legalAccessEvent } from '@erpx/server-collaboration/adapters/native-legal-access-event';
import { nativeLegalAuxiliaryRead } from '../native-legal-auxiliary';
import { legalStatePointSql } from '../legal-read-tail-r33-sources';
import { legalActiveDestructionGuardHead } from '../legal-read-tail-r46-sources';
/** Shared by search and direct citations; aliases must be legal_documents d and legal_items i.
 * Retained for the command-side availability check in `citations.ts` and the AI evidence reads. */
export const legalEvidenceDocumentAvailableSql = `(d.object_key IS NULL OR EXISTS(SELECT 1 FROM (${filesProjectionSources.attachment_links}) clean_link JOIN (${filesProjectionSources.attachments}) clean_file ON clean_file.id=clean_link.attachment_id WHERE clean_link.id=d.attachment_link_id AND clean_link.state='bound' AND clean_file.company_id=i.company_id AND clean_file.source_state='available' AND clean_file.revoked_at IS NULL AND clean_file.checksum_sha256=d.checksum)) AND NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=d.id) ${legalActiveDestructionGuardHead()}`;
const cleanDocument = legalEvidenceDocumentAvailableSql;
/** Exact projection of the native `knowledge` auxiliary read; the transport accepts
 * every authoritative field and no view SQL remains in TypeScript. */
type LegalKnowledgeView = { sources: LegalEvidenceSource[]; conflicts: number; authoredItems?: LegalEvidenceSource[] };
/** Exact projection of the native `law-citations` auxiliary read. */
type LegalLawCitation = LegalEvidenceSource;
function knowledgeParams(q: string, asOf?: string, itemId?: string) { const p = new URLSearchParams({ q }); if (asOf) p.set("asOf", asOf); if (itemId) p.set("itemId", itemId); return p; }
function citationParams(asOf?: string) { const p = new URLSearchParams(); if (asOf) p.set("asOf", asOf); return p; }
function application(db:SqlDatabase,a:WorkContext,view?:LegalKnowledgeView,citation?:LegalLawCitation){const v=visibility(a);let observation:ReturnType<typeof legalReadScope>|undefined;const purchase=async()=>{const scope=await(observation??=legalReadScope(db,a));return scope.purchases?.["legal.documents"]?.history===false?{...scope,where:"FALSE"}:scope;};return createLegalKnowledgeApplication({require:()=>requirePermission(a,"legal.read"),access:id=>access(db,a,id,"legal.ai"),document:id=>documentAccess(db,a,id),error:(status,code,message)=>new ApiError(status,code,message),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},repository:{
 lawSources:async()=>(view?.sources??[]).filter(source=>source.kind==="law"),
 lawConflicts:async()=>view?.conflicts??0,
 lawSource:async(id,asOf)=>citation??await nativeLegalAuxiliaryRead<LegalEvidenceSource>(a,"citations",`law:${id.replace(/^law:/,"")}`,citationParams(asOf)),
 reviewed:async()=>(view?.sources??[]).filter(source=>source.kind==="document"),
 authored:async()=>[],
 authoredItem:async()=>view?.authoredItems??[],
 documentAvailable:async id=>{const d={id};return Boolean(await db.prepare(`SELECT 1 FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND ${v.sql} AND ${(await purchase()).where} AND ${cleanDocument}`).bind(d.id, ...v.values).first());},
 textApproved:async(reviewId,documentId)=>{const source={text_review_id:reviewId},d={id:documentId};return Boolean(await db.prepare(legalStatePointSql("1","legal_text_reviews","document_id","approved")).bind(source.text_review_id, d.id).first());},
 recordReads:async rows=>{const ai=rows.filter(row=>row.action==="ai.source.read");if(ai.length){const statements=await Promise.all(ai.map(source=>legalAccessEvent(db,a.companyId,a.userId).record(source.identifier,source.action,source.item_id,source.document_id,source.now)));await db.batch(statements);}}
 }});}
export async function lawKnowledge(db:SqlDatabase,a:WorkContext,query:unknown,asOf?:unknown){const q=String(query),date=asOf==null?undefined:String(asOf),view=await nativeLegalAuxiliaryRead<LegalKnowledgeView>(a,"knowledge",undefined,knowledgeParams(q,date));return application(db,a,view).lawKnowledge(query,asOf);}
export async function lawCitation(db:SqlDatabase,a:WorkContext,id:string,asOf?:unknown){const date=asOf==null?undefined:String(asOf),citation=await nativeLegalAuxiliaryRead<LegalLawCitation>(a,"citations",`law:${id.replace(/^law:/,"")}`,citationParams(date));return application(db,a,undefined,citation).lawCitation(id,asOf);}
export async function legalKnowledge(db:SqlDatabase,a:WorkContext,query:unknown,asOf?:unknown){const q=String(query),date=asOf==null?undefined:String(asOf),view=await nativeLegalAuxiliaryRead<LegalKnowledgeView>(a,"knowledge",undefined,knowledgeParams(q,date));return application(db,a,view).legalKnowledge(query,asOf);}
export async function retrieveLegalAiEvidence(db:SqlDatabase,a:WorkContext,itemId:string,question:string){const q=question.slice(0,200),view=await nativeLegalAuxiliaryRead<LegalKnowledgeView>(a,"knowledge",undefined,knowledgeParams(q,undefined,itemId));return application(db,a,view).retrieveLegalAiEvidence(itemId,question);}
export function revalidateLegalAiEvidence(db:SqlDatabase,a:WorkContext,evidence:LegalAiEvidence){return application(db,a).revalidateLegalAiEvidence(evidence);}
export function validateLegalAiCitations<T extends {sources:string[];claims:{text:string;sources:string[];quotes?:{sourceId:string;quote:string}[]}[]}>(proposal:T,evidence:LegalAiEvidence){return validateCitations(proposal,evidence,(status,code,message)=>new ApiError(status,code,message));}
