

import 'server-only';

import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { MessagingRoom } from "@erpx/feature-collaboration/lib/messaging";

import { BusinessRuleError } from '@erpx/server-shared/index';
import { ApiError } from '@erpx/server-runtime/public/http';

import { requireMessagingPurchase } from '@erpx/server-collaboration/adapters/messaging-purchases';
import {messagingRoomAccessSql} from '@erpx/server-collaboration/adapters/messaging-source-policy';
import{createMessagingRead,type createMessagingReadPorts}from '@erpx/server-collaboration/application/messaging';
const access=messagingRoomAccessSql();
export function nativeMessagingReadRepository(db:SqlDatabase):createMessagingReadPorts<SqlStatement>['repository']{return{async room(id: string, companyId: string, value: string[]) {
            return db
                .prepare(`SELECT r.* FROM messaging_rooms r WHERE r.id=? AND r.company_id=? AND ${access}`)
                .bind(id, companyId, ...value)
                .first<MessagingRoom>();
        }};}
function application(db: SqlDatabase){return createMessagingRead({repository:{room:(...args)=>nativeMessagingReadRepository(db).room(...args)},});}
async function exposed<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function messagingRoom(db:SqlDatabase,actor:WorkContext,id:string){await requireMessagingPurchase(db,actor,'history');return exposed(()=>application(db).messagingRoom(actor,id));}
