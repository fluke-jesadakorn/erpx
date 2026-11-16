import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function clearAgentBootstrapAssertion(db:SqlDatabase,key:string){return db.prepare("SELECT 1").bind();}
