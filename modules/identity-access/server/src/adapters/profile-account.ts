import "server-only";
import {runtimeEnv} from "../../../../../platform/runtime/server/src/public/runtime";
import {userAttributeByIdSql} from './identity-user-projection-family-r18';
export async function memberProfileAccount(userId:string){return (await runtimeEnv()).CONTROL_DB.prepare(userAttributeByIdSql('id,name')).bind(userId).first<{id:string;name:string}>();}
