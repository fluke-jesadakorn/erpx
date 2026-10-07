import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function operationsLevelHistoryEffects(db:SqlDatabase){return {stageChange:(value:string,engagementId:string,previous:string|null,levelId:string,userId:string,reason:string,key:string,now:number)=>db.prepare("INSERT INTO staff_level_history(id,engagement_id,previous_level_id,level_id,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(value,engagementId,previous,levelId,userId,reason,key,now)};}
