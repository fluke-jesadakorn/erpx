import "server-only";
import { createHash } from "node:crypto";
import type { EventMessage } from "@erpx/server-workflow/public/job-projector";
import { isTransactionalSqlDatabase, type SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { pmsSocialOriginal } from "@erpx/server-workflow/public/pms-social-event-facts";
import { pmsSocialWrites } from "@erpx/server-collaboration/public/pms-social-event-facts";
import { pmsSocialRecipients } from "@erpx/server-identity_access/public/pms-social-event-recipients";
import { pmsSourcesSql } from "@erpx/server-pms/adapters/pms/source-query";
import type { PmsSource } from "@erpx/server-pms/domain/source";
import { createPmsSocialProjector, type PmsSocialUnit } from "@erpx/server-pms/application/social-events";
function unit(db: SqlDatabase): PmsSocialUnit {
    const social = pmsSocialWrites(db);
    return {
        original: (id, type) => pmsSocialOriginal(db, id, type),
        payoutSource: (id, propertyId) => db.prepare("SELECT data_json::jsonb->>'originalId' id FROM pms_records WHERE id=? AND property_id=? AND kind='entry'").bind(id, propertyId).first<string>("id"),
        source: (id, companyId, propertyId) => db.prepare(`SELECT pr.* FROM (${pmsSourcesSql}) pr WHERE pr.id=? AND pr.company_id=? AND pr.property_id=?`).bind(id, companyId, propertyId).first<PmsSource>(),
        lock: postId => db.prepare("SELECT pg_advisory_xact_lock(hashtext(?))").bind(postId).first(),
        received: eventId => db.prepare("SELECT 1 FROM pms_social_event_receipts WHERE event_id=?").bind(eventId).first(),
        publish: async (p) => { await db.batch([social.post(p), social.source(p.postId, p.sourceId), db.prepare("INSERT INTO pms_social_event_receipts(event_id,post_id,created_at) VALUES(?,?,?)").bind(p.eventId, p.postId, p.receivedAt)]); },
        recipients: (...args) => pmsSocialRecipients(db, ...args), version: social.version, notify: social.notify
    };
}
export function projectPmsSocialEvent(db: SqlDatabase, message: EventMessage) { return createPmsSocialProjector({ transaction: work => { if (!isTransactionalSqlDatabase(db))
        throw new Error("PMS_SOCIAL_TRANSACTION_REQUIRED"); return db.transaction(tx => work(unit(tx))); }, clock: { now: () => Date.now() }, hash: value => createHash("sha256").update(value).digest("hex") }).projectPmsSocialEvent(message); }
