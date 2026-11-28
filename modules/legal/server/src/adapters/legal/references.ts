import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {getOrder} from '@erpx/commerce-server/public/commerce-order-read';
import {legalPersonSource} from "@erpx/people-payroll-server/public/legal-evidence-read";
import {getEmployeeRequest} from '@erpx/records-server/public/employee-requests-read';
import {readExpenseForLegal} from "@erpx/people-payroll-server/public/legal-expense-read";
import {access,guards} from "@erpx/legal-server/adapters/legal/access";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import {createLegalReferencesApplication} from "@erpx/legal-server/application/references";
import type {LegalReferenceKind} from "@erpx/legal-server/domain/references";
import { legalSigningReferences } from '@erpx/collaboration-server/adapters/native-legal-signing-references';
import {nativeLegalAuxiliaryRead} from '../native-legal-auxiliary';
import {legalRateCountPrefix} from '../legal-read-tail-r19-sources';
/** Exact projection of the native `references` auxiliary read; no view SQL remains in TypeScript. */
type LegalReferencesView={target_module:LegalReferenceKind;target_id:string}[];
function application(db:SqlDatabase,a:WorkContext,view?:LegalReferencesView){const native=legalSigningReferences(db,a.companyId,a.userId);return createLegalReferencesApplication({access:(id,p)=>access(db,a,id,p),requirePeople:()=>requirePermission(a,"people.read"),target:async(kind,id)=>{if(kind==="person")return legalPersonSource(db,a,id);if(kind==="commerce_order")return (await getOrder(a,id,db)).order; if(kind==="employee_request")return await getEmployeeRequest(db,a,id) as unknown as Record<string,unknown>;return await readExpenseForLegal(a,id,db) as unknown as Record<string,unknown>;},clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},repository:{
 list:async()=>view??[],
 replay:(key,id,raw)=>replayCommand(db,a,key,"legal.reference",id,raw),
 commit:async (x,item,raw,key,times,eventId)=>executeCommand(db,a,key,"legal.reference",raw,item.id,item.version+1,{id:item.id,version:item.version+1},[
 ...guards(db,a,item.id,x.expectedVersion,key,true),assertion(db,key,"reference-limit",`${legalRateCountPrefix}native_references WHERE item_id=?)<50`,[item.id]),
 await native.addReference({ itemId: item.id, targetKind: x.targetKind, targetId: x.targetId, createdBy: a.userId, createdAt: times.created }),
 await native.itemAdvance({ updated: times.updated, itemId: item.id }),
 await native.event({ id: eventId, action: "reference.added", note: `${x.targetKind}:${x.targetId}`, version: item.version+1, identifier: item.id, now: times.event }),
 ]),}});}
export async function legalReferences(db:SqlDatabase,a:WorkContext,id:string){const view=await nativeLegalAuxiliaryRead<LegalReferencesView>(a,'references',id);return application(db,a,view).legalReferences(id);}
export function addLegalReference(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).addLegalReference(raw,key);}
