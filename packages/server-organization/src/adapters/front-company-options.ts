import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export async function frontActiveCompanies(db:SqlDatabase,ids:string[]){if(!ids.length)return [];return (await db.prepare("SELECT id,slug FROM organization_nodes WHERE kind='company' AND status='active' AND id IN ("+ids.map(()=>"?").join(",")+")").bind(...ids).all<{id:string;slug:string}>()).results;}
