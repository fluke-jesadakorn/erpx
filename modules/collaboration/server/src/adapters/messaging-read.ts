

import 'server-only';

import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { MessagingRoom } from "@erpx/collaboration-ui/lib/messaging";

import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

import { requireMessagingPurchase } from '@erpx/collaboration-server/adapters/messaging-purchases';
import {messagingRoomAccessSql} from '@erpx/collaboration-server/adapters/messaging-source-policy';
import{createMessagingRead,type createMessagingReadPorts}from '@erpx/collaboration-server/application/messaging';
import {messagingRoomByIdSql} from './read-tail-sql';
const access=messagingRoomAccessSql();
export function nativeMessagingReadRepository(db:SqlDatabase):createMessagingReadPorts<SqlStatement>['repository']{return{async room(id: string, companyId: string, value: string[]) {
            return db
                .prepare(messagingRoomByIdSql('r.*',access))
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
