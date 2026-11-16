import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function recordPeoplePlacementLevelHistory(db:SqlDatabase,id:string,engagementId:string,previousLevelId:string|null,levelId:string|null,actorId:string,reason:string,commandId:string,now:number){return db.prepare("SELECT 1").bind().run();}
