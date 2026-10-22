import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export const workLayoutClearAssertions=(db:SqlDatabase,key:string)=>db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key);
