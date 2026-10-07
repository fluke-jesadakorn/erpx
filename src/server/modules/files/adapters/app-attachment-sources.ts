import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export type AppReservedUpload = { declared_bytes: number; media_type: string; attachment_set_id: string; owner_type: string; purpose: string; component_kind: "record" | "action" | "line" | "event" | "message" | "publication"; component_ref: string | null; purpose_key: string; expected_target_version: number; compatibility_adapter: boolean };
/** Endpoint adapters read owner facts on their already authorized supplied database. */
export function appAttachmentSources(db: SqlDatabase) { return {
  portalReservation: (id: string, userId: string, companyId: string) => db.prepare(`SELECT declared_bytes,media_type FROM upload_intents
      WHERE id=? AND attachment_set_id IS NOT NULL AND created_by=? AND company_id=?`).bind(id, userId, companyId).first<{ declared_bytes: number; media_type: string }>(),
  previousObject: (id: string) => db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(id).first<{ object_key: string }>(),
  finalReconciliation: () => db.prepare("SELECT ready FROM attachment_reconciliation_runs WHERE mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC LIMIT 1").first<{ ready: boolean }>(),
  canonicalLinks: async (id: string, companyId: string) => (await db.prepare(`SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
      WHERE l.attachment_id=? AND s.company_id=? AND l.state IN ('draft','bound') ORDER BY CASE l.state WHEN 'bound' THEN 0 ELSE 1 END,l.created_at`).bind(id, companyId).all<{ id: string }>()).results,
  compatibilitySet: async (setId: string, companyId: string) => Boolean(await db.prepare(`SELECT 1 FROM attachment_sets set_row
           JOIN attachment_events event ON event.set_id=set_row.id AND event.company_id=set_row.company_id
           WHERE set_row.id=? AND set_row.company_id=? AND event.action='set.created'
             AND COALESCE((event.details_json::jsonb->>'compatibilityAdapter')::boolean,false)=true
           LIMIT 1`).bind(setId, companyId).first()),
  universalReservation: (id: string, userId: string, companyId: string) => db.prepare(`SELECT u.declared_bytes,u.media_type,u.attachment_set_id,u.owner_type,u.purpose,
           s.component_kind,s.component_ref,s.purpose_key,s.expected_target_version,
           EXISTS(SELECT 1 FROM attachment_events event WHERE event.set_id=s.id AND event.company_id=s.company_id
             AND event.action='set.created' AND COALESCE((event.details_json::jsonb->>'compatibilityAdapter')::boolean,false)=true) compatibility_adapter
         FROM upload_intents u
         JOIN attachment_sets s ON s.id=u.attachment_set_id
         WHERE u.id=? AND u.created_by=? AND u.company_id=?`).bind(id, userId, companyId).first<AppReservedUpload>(),
}; }
