import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import {access,guards} from "@erpx/legal-server/adapters/legal/access";
import {refuseLegalSigning} from "@erpx/legal-server/domain/signing";
import {createLegalSigningApplication,type LegalEnvelopeView} from "@erpx/legal-server/application/signing";
import { legalSigningReferences } from '@erpx/collaboration-server/adapters/native-legal-signing-references';
import { legalExistsPointGuardHeadSql } from '../legal-read-tail-r40-sources';
import { legalEnvelopeSignerReadHead } from '../legal-read-tail-r43-sources';
export function assertLegalSigningEnabled():never{return refuseLegalSigning((status,code,message)=>new ApiError(status,code,message));}
function application(db:SqlDatabase,a:WorkContext){const native=legalSigningReferences(db,a.companyId,a.userId);return createLegalSigningApplication({access:(id,p)=>access(db,a,id,p),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
 envelope:(envelopeId,itemId)=>db.prepare(legalEnvelopeSignerReadHead("e.*,s.ordinal,s.state signer_state,s.viewed_at")+"e.id=? AND e.item_id=? AND s.user_id=?").bind(envelopeId,itemId,a.userId).first<LegalEnvelopeView>(),
 replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),
 view:async (item,e,key,x,times,eventId)=>executeCommand(db,a,key,"legal.signing.view",x,item.id,item.version+1,{id:item.id,version:item.version+1,state:item.state},[
 ...guards(db,a,item.id,item.version,key,false),assertion(db,key,"legal-envelope",`${legalExistsPointGuardHeadSql('legal_envelopes','id')}state='sent' AND expires_at>?)`,[e.id,times.envelope]),
 await native.signerView({ viewed: times.viewed, envelopeId: e.id }),
 await native.itemStateAdvance({ state: item.state, updated: times.updated, itemId: item.id }),
 await native.event({ id: eventId, action: "signing.view", note: e.document_hash, version: item.version+1, identifier: item.id, now: times.event }),
 ]),}});}
export function sendEnvelope(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).sendEnvelope(raw,key);}
export function signingAction(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).signingAction(raw,key);}
