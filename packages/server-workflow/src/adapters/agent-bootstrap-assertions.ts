import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function clearAgentBootstrapAssertion(db:SqlDatabase,key:string){return db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key);}
