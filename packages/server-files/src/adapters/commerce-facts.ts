import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/hospitality-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type { Row } from "@erpx/server-commerce/public/commerce-contracts";
export function CommerceFilesFacts(db: SqlDatabase) {
    return {
        readAttachments1: (facts: {
            id: string;
        }) => db.prepare(`SELECT a.id attachment_id,a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.created_at FROM attachments a WHERE a.owner_type='commerce_order_delivery' AND a.owner_id=? AND a.purpose='proof_of_delivery' AND a.revoked_at IS NULL AND NOT EXISTS(SELECT 1 FROM attachment_links canonical WHERE canonical.attachment_id=a.id) AND NOT EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_fulfillment_evidence}) e WHERE e.attachment_id=a.id) ORDER BY a.created_at DESC,a.id`).bind(facts.id).all<Row>(),
        readAttachments2: (facts: {
            attachmentId: string;
            id: string;
        }) => db.prepare(`SELECT 1 ok FROM attachments a WHERE a.id=? AND a.owner_type='commerce_order_delivery' AND a.owner_id=? AND a.purpose='proof_of_delivery' AND a.revoked_at IS NULL AND NOT EXISTS(SELECT 1 FROM attachment_links canonical WHERE canonical.attachment_id=a.id) AND NOT EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_fulfillment_evidence}) e WHERE e.attachment_id=a.id)`).bind(facts.attachmentId, facts.id).first('ok'),
        readAttachmentSets1: (facts: {
            companyId: string;
            invoiceId: string;
            expectedVersion: number;
            paymentSlipId: string;
        }) => db.prepare(`SELECT s.id FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id
      WHERE s.company_id=? AND s.target_type='invoice_payment' AND s.target_id=?
        AND s.component_kind='action' AND s.component_ref='legacy-upload-adapter' AND s.purpose_key='payment_slip'
        AND s.expected_target_version=? AND s.state='draft' AND l.state='draft' AND l.attachment_id=?
      ORDER BY s.created_at,s.id LIMIT 1`).bind(facts.companyId, facts.invoiceId, facts.expectedVersion, facts.paymentSlipId).first<{
            id: string;
        }>(),
        guardRule23: (facts: {
            key: string;
            guardId: string;
            values: string[];
        }) => assertion(db, facts.key, facts.guardId, `EXISTS(SELECT 1 FROM attachments a WHERE a.id=? AND a.owner_type='commerce_order_delivery' AND a.owner_id=? AND a.purpose='proof_of_delivery' AND a.revoked_at IS NULL AND NOT EXISTS(SELECT 1 FROM attachment_links canonical WHERE canonical.attachment_id=a.id) AND NOT EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_fulfillment_evidence}) e WHERE e.attachment_id=a.id))`, facts.values),
        guardRule32: (facts: {
            key: string;
            guardId: string;
            values: (string | null)[];
        }) => assertion(db, facts.key, facts.guardId, "EXISTS(SELECT 1 FROM attachments WHERE id=? AND owner_type='invoice_payment' AND owner_id=? AND revoked_at IS NULL AND media_type IN ('image/jpeg','image/png','image/webp','application/pdf'))", facts.values)
    };
}
