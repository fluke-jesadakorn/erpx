import type { SqlDatabase } from '../../runtime/public/runtime/types';
export async function collaborationMentionPrincipals(control: SqlDatabase, ids: string[]) { return (await control.prepare(`SELECT user_id,display_name FROM principals WHERE status='active' AND user_id IN (${ids.map(() => '?').join(',')}) ORDER BY display_name,user_id`).bind(...ids).all<{
    user_id: string;
    display_name: string;
}>()).results; }
