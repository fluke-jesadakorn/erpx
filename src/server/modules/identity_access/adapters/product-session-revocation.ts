import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';

/** The original SSO family deletion protocol. Metadata is retained; every
 * consumer still joins the live parent session. The caller owns the unit. */
export async function revokeProductSessionFamily(database:SqlDatabase,sessionId:string,environment:'development'){
 const row=await database.prepare('SELECT * FROM sso_session_bindings WHERE session_id=?').bind(sessionId).first<{parent_session_id:string|null;environment:string}>();
 if(!row||row.environment!==environment)return;
 const parent=row.parent_session_id??sessionId;
 await database.prepare('SELECT id FROM session WHERE id=? FOR UPDATE').bind(parent).first();
 await database.prepare('DELETE FROM session WHERE id=? OR id IN (SELECT session_id FROM sso_session_bindings WHERE parent_session_id=? AND environment=?)').bind(parent,parent,environment).run();
}
