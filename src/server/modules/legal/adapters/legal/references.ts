import "server-only";
import type {SqlDatabase} from "../../../runtime/public/runtime/types";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {requirePermission} from "../../../identity_access/public/iam-permissions";
import {getOrder} from "../../../commerce/public/commerce";
import {legalPersonSource} from "../../../people_payroll/public/legal-evidence-read";
import {getEmployeeRequest} from "../../../records/public/employee-requests";
import {readExpenseForLegal} from "../../../people_payroll/public/legal-expense-read";
import {access,guards} from "./access";
import {executeCommand,replayCommand,assertion} from "../../../workflow/public/commands";
import {createLegalReferencesApplication} from "../../application/references";
import type {LegalReferenceKind} from "../../domain/references";
function application(db:SqlDatabase,a:WorkContext){return createLegalReferencesApplication({access:(id,p)=>access(db,a,id,p),requirePeople:()=>requirePermission(a,"people.read"),target:async(kind,id)=>{if(kind==="person")return legalPersonSource(db,a,id);if(kind==="commerce_order")return (await getOrder(a,id,db)).order; if(kind==="employee_request")return await getEmployeeRequest(db,a,id) as unknown as Record<string,unknown>;return await readExpenseForLegal(a,id,db) as unknown as Record<string,unknown>;},clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},repository:{
 list:async id=>(await db.prepare("SELECT target_module,target_id FROM legal_native_references WHERE item_id=? ORDER BY ordinal LIMIT 50").bind(id).all<{target_module:LegalReferenceKind;target_id:string}>()).results,
 replay:(key,id,raw)=>replayCommand(db,a,key,"legal.reference",id,raw),
 commit:(x,item,raw,key,times,eventId)=>executeCommand(db,a,key,"legal.reference",raw,item.id,item.version+1,{id:item.id,version:item.version+1},[
 ...guards(db,a,item.id,x.expectedVersion,key,true),assertion(db,key,"reference-limit","(SELECT count(*) FROM legal_native_references WHERE item_id=?)<50",[item.id]),
 db.prepare("INSERT INTO legal_native_references(item_id,target_module,target_id,ordinal,created_by,created_at) VALUES(?,?,?,(SELECT coalesce(max(ordinal),-1)+1 FROM legal_native_references WHERE item_id=?),?,?) ON CONFLICT DO NOTHING").bind(item.id,x.targetKind,x.targetId,item.id,a.userId,times.created),
 db.prepare("UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?").bind(times.updated,item.id),
 db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)").bind(eventId,item.id,"reference.added",a.userId,`${x.targetKind}:${x.targetId}`,item.version+1,times.event),
 ]),}});}
export function legalReferences(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).legalReferences(id);}
export function addLegalReference(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).addLegalReference(raw,key);}
