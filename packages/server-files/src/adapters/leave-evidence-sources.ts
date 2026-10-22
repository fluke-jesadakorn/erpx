import { lastAuditOwnerProjectionSources as people_payrollLastAuditProjectionSources } from "@erpx/server-people_payroll/public/last-audit-owner-projection-sources";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';export function leaveEvidenceSources(database:SqlDatabase){return {availableCanonical(leaveRequestId:string,purposeKey:"medical_certificate" | "leave_supporting_photo"){return database.prepare(
	        `SELECT DISTINCT set_row.id,source.id source_id
	         FROM attachment_sets set_row
	         JOIN (${people_payrollLastAuditProjectionSources.leave_requests}) request ON request.id=set_row.target_id
	           AND request.company_id=set_row.company_id
	         JOIN attachment_links link ON link.set_id=set_row.id
	           AND link.state IN ('draft','bound')
	         JOIN attachments source ON source.id=link.attachment_id
	           AND source.company_id=set_row.company_id
	           AND source.source_state='available'
         WHERE set_row.target_type='leave_request' AND set_row.target_id=?
           AND set_row.purpose_key=? AND set_row.state IN ('draft','bound')
         ORDER BY set_row.id,source.id`,
      )
      .bind(leaveRequestId, purposeKey)
      .all<{ id: string; source_id: string }>();},
availableLegacy(leaveRequestId:string,purposeKey:"medical_certificate" | "leave_supporting_photo"){return database.prepare(
	          `SELECT source.id
	           FROM attachments source
	           JOIN (${people_payrollLastAuditProjectionSources.leave_requests}) request ON request.id=source.owner_id
	             AND request.company_id=source.company_id
	           WHERE source.owner_type='leave_request' AND source.owner_id=?
             AND source.revoked_at IS NULL AND source.purpose=?
             AND source.source_state='available'
             AND NOT EXISTS(
               SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id
             )`,
        )
        .bind(leaveRequestId, purposeKey)
        .all<{ id: string }>();},
canonicalManifest(leaveRequestId:string){return database.prepare(
        `SELECT set_row.id set_id,set_row.state set_state,set_row.purpose_key,link.id link_id,
           source.id source_id,source.checksum_sha256,source.detected_media_type,
           source.media_type,source.bytes
	         FROM attachment_sets set_row
	         JOIN (${people_payrollLastAuditProjectionSources.leave_requests}) request ON request.id=set_row.target_id
	           AND request.company_id=set_row.company_id
	         JOIN attachment_links link ON link.set_id=set_row.id
	         JOIN attachments source ON source.id=link.attachment_id
	           AND source.company_id=set_row.company_id
         WHERE set_row.target_type='leave_request' AND set_row.target_id=?
           AND set_row.state IN ('draft','bound')
           AND link.state IN ('draft','bound')
           AND source.source_state='available'
         ORDER BY set_row.created_at,link.created_at,link.id`,
      )
      .bind(leaveRequestId)
      .all<{
        set_id: string;
        set_state: "draft" | "bound";
        purpose_key: string;
        link_id: string;
        source_id: string;
        checksum_sha256: string;
        detected_media_type: string | null;
        media_type: string;
        bytes: number;
      }>();},
legacyManifest(leaveRequestId:string){return database.prepare(
          `SELECT source.id,source.checksum_sha256,source.detected_media_type,
             source.media_type,source.bytes,source.purpose
	           FROM attachments source
	           JOIN (${people_payrollLastAuditProjectionSources.leave_requests}) request ON request.id=source.owner_id
	             AND request.company_id=source.company_id
           WHERE source.owner_type='leave_request' AND source.owner_id=?
             AND source.revoked_at IS NULL AND source.source_state='available'
             AND NOT EXISTS(
               SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id
             )
           ORDER BY source.created_at,source.id`,
        )
        .bind(leaveRequestId)
        .all<{
          id: string;
          checksum_sha256: string;
          detected_media_type: string | null;
          media_type: string;
          bytes: number;
          purpose: string;
        }>();},
pendingEvidence(leaveRequestId:string){return database.prepare(
      `SELECT count(*)::int count
	       FROM attachment_sets set_row
	       JOIN (${people_payrollLastAuditProjectionSources.leave_requests}) request ON request.id=set_row.target_id
	         AND request.company_id=set_row.company_id
	       JOIN attachment_links link ON link.set_id=set_row.id
	       LEFT JOIN attachments source ON source.id=link.attachment_id
	         AND source.company_id=set_row.company_id
         LEFT JOIN upload_intents intent ON intent.id=link.upload_intent_id
           AND intent.company_id=set_row.company_id
       WHERE set_row.target_type='leave_request' AND set_row.target_id=?
         AND set_row.state='draft' AND link.state='draft'
         AND (
           (link.attachment_id IS NULL AND intent.id IS NOT NULL)
           OR (source.id IS NOT NULL AND source.source_state<>'available')
         )`,
    )
    .bind(leaveRequestId)
    .first<{ count: number }>();}};}
