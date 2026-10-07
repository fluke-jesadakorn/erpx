import "server-only";
import {runtimeEnv} from "../../runtime/public/runtime";
export async function memberProfileAccount(userId:string){return (await runtimeEnv()).CONTROL_DB.prepare('SELECT id,name FROM "user" WHERE id=?').bind(userId).first<{id:string;name:string}>();}
