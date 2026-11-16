import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function calendarRetirementNotification(db:SqlDatabase,facts:{value1:string;value2:string;value3:string;value4:string;value5:number;value6:string;value7:number}){return db.prepare("SELECT 1").bind().run();}
