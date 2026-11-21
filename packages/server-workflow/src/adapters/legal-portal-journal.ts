import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {workflowWave3Statement} from "./native-workflow-wave3";
export function legalPortalJournal(db:SqlDatabase){return {
audit:async(facts:{identifier:string;key:string;actorId:string;fact3:"external-share";action:string;fact5:"legal";itemId:string;version:number;fact8:"Shared document interaction";now:number;authority:string;})=>(await workflowWave3Statement(db,{kind:'system'},'wf_portal_audit',[facts.identifier,facts.key,facts.actorId,facts.fact3,facts.action,facts.fact5,facts.itemId,facts.version,facts.fact8,facts.now,facts.authority],String(facts.key).slice(0,180),facts.now)).run(),
outbox:async(facts:{identifier:string;key:string;fact2:string;fact3:"legal";itemId:string;version:number;fact6:string;now:number;})=>(await workflowWave3Statement(db,{kind:'system'},'wf_portal_outbox',[facts.identifier,facts.key,facts.fact2,facts.fact3,facts.itemId,facts.version,facts.fact6,facts.now],String(facts.key).slice(0,180),facts.now)).run()
};}
