import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { PmsSocialOriginal } from "../../pms/public/social-event-contracts";
export function pmsSocialOriginal(db: SqlDatabase, id: string, eventType: string) {
    return db.prepare(`SELECT e.command_id,e.created_at,c.actor_id,c.result_json
      FROM outbox_events e JOIN command_receipts c ON c.command_id=e.command_id
      WHERE e.id=? AND e.event_type=?`).bind(id, eventType).first<PmsSocialOriginal>();
}
