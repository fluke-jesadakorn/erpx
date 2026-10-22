import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function legalPortalJournal(db:SqlDatabase){return {
audit:(facts:{identifier:string;key:string;actorId:string;fact3:"external-share";action:string;fact5:"legal";itemId:string;version:number;fact8:"Shared document interaction";now:number;authority:string;})=>db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(facts.identifier,facts.key,facts.actorId,facts.fact3,facts.action,facts.fact5,facts.itemId,facts.version,facts.fact8,facts.now,facts.authority).run(),
outbox:(facts:{identifier:string;key:string;fact2:string;fact3:"legal";itemId:string;version:number;fact6:string;now:number;})=>db.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(facts.identifier,facts.key,facts.fact2,facts.fact3,facts.itemId,facts.version,facts.fact6,facts.now).run()
};}
