import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {principalDisplaySql} from './read-tail-family-r27';
export async function collaborationMentionPrincipals(control: SqlDatabase, ids: string[]) { return (await control.prepare(`${principalDisplaySql(`user_id IN (${ids.map(() => '?').join(',')})`)} ORDER BY display_name,user_id`).bind(...ids).all<{
    user_id: string;
    display_name: string;
}>()).results; }
