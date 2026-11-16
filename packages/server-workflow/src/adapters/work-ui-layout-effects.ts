import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export const workLayoutClearAssertions=(db:SqlDatabase,key:string)=>db.prepare("SELECT 1").bind();
