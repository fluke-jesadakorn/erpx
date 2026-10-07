import "server-only";
import type {SqlDatabase} from "../../../runtime/public/runtime/types";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {ApiError} from "../../../runtime/public/http";
import {executeCommand,assertion,replayCommand} from "../../../workflow/public/commands";
import {access,guards} from "./access";
import {refuseLegalSigning} from "../../domain/signing";
import {createLegalSigningApplication,type LegalEnvelopeView} from "../../application/signing";
export function assertLegalSigningEnabled():never{return refuseLegalSigning((status,code,message)=>new ApiError(status,code,message));}
function application(db:SqlDatabase,a:WorkContext){return createLegalSigningApplication({access:(id,p)=>access(db,a,id,p),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
 envelope:(envelopeId,itemId)=>db.prepare("SELECT e.*,s.ordinal,s.state signer_state,s.viewed_at FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.id=? AND e.item_id=? AND s.user_id=?").bind(envelopeId,itemId,a.userId).first<LegalEnvelopeView>(),
 replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),
 view:(item,e,key,x,times,eventId)=>executeCommand(db,a,key,"legal.signing.view",x,item.id,item.version+1,{id:item.id,version:item.version+1,state:item.state},[
 ...guards(db,a,item.id,item.version,key,false),assertion(db,key,"legal-envelope","EXISTS(SELECT 1 FROM legal_envelopes WHERE id=? AND state='sent' AND expires_at>?)",[e.id,times.envelope]),
 db.prepare("UPDATE legal_signers SET viewed_at=? WHERE envelope_id=? AND user_id=?").bind(times.viewed,e.id,a.userId),
 db.prepare("UPDATE legal_items SET version=version+1,state=?,updated_at=? WHERE id=?").bind(item.state,times.updated,item.id),
 db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)").bind(eventId,item.id,"signing.view",a.userId,e.document_hash,item.version+1,times.event),
 ]),}});}
export function sendEnvelope(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).sendEnvelope(raw,key);}
export function signingAction(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).signingAction(raw,key);}
