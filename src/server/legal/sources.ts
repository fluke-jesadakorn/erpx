import "server-only";
import { z } from "zod";
import type { PostgresDatabase } from "../runtime/postgres";
import type { FileStore } from "../runtime/types";
import type { WorkContext } from "../authorization";
import { getEmployeeRequest } from "../employee-requests";
import { getTask } from "../staff-tasks";
import { getExpense } from "../expenses";
import { getLeave } from "../leave";
import { paymentDraft } from "../expense-payments";
import { invoicePaymentOwner } from "../invoice-payment-evidence";
import { uploadDocument } from "./documents";
import { access, fail } from "./access";
import { hash } from "./service";
import { replayCommand } from "../commands";
import {
  attachmentsV2Readable,
  attachmentsV2ReadOnly,
} from "../attachments/feature";
import { attachmentContent } from "../attachments/service";
export async function preserveAttachment(
  db: PostgresDatabase,
  files: FileStore,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.literal("source.attach"),
      id: z.string(),
      expectedVersion: z.number().int().positive(),
      attachmentId: z.string(),
    })
    .parse(raw);
  if (attachmentsV2ReadOnly())
    fail(
      "Attachment preservation is disabled during attachment rollback.",
      "ATTACHMENTS_READ_ONLY",
      503,
    );
  await access(db, a, x.id, "legal.edit");
  return db.transaction(async (tx) => {
    await tx
      .prepare(
        "SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))",
      )
      .run();
    const row = await tx
      .prepare(
        `SELECT a.*,(SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
          WHERE l.attachment_id=a.id AND s.company_id=a.company_id AND s.target_type=a.owner_type AND s.target_id=a.owner_id
            AND l.state IN ('draft','bound') ORDER BY CASE l.state WHEN 'bound' THEN 0 ELSE 1 END,l.created_at LIMIT 1) attachment_link_id
         FROM attachments a WHERE a.id=? AND a.company_id=? AND a.revoked_at IS NULL`,
      )
      .bind(x.attachmentId, a.companyId)
      .first<{
        object_key: string;
        media_type: string;
        original_name: string;
        owner_type: string;
        owner_id: string;
        checksum_sha256: string;
        source_state: string;
        attachment_link_id: string | null;
      }>();
    if (!row)
      fail("Source attachment unavailable.", "LEGAL_SOURCE_NOT_FOUND", 404);
    switch (row.owner_type) {
      case "employee_request":
        await getEmployeeRequest(tx, a, row.owner_id);
        break;
      case "staff_task":
        await getTask(tx, a, row.owner_id);
        break;
      case "expense_claim":
        await getExpense(a, row.owner_id);
        break;
      case "leave_request":
        await getLeave(a.holdingId, row.owner_id, a);
        break;
      case "expense_payment":
        await paymentDraft(a, row.owner_id);
        break;
      case "invoice_payment":
        await invoicePaymentOwner(tx, a, row.owner_id);
        break;
      default:
        fail(
          "This attachment owner is not supported for evidence preservation.",
        );
    }
    if (
      attachmentsV2Readable() &&
      (row.source_state !== "available" || !row.attachment_link_id)
    )
      fail(
        "Source attachment is not available through an authorized clean file link.",
        "LEGAL_SOURCE_NOT_FOUND",
        404,
      );
    const canonical =
      attachmentsV2Readable() && row.attachment_link_id
        ? await attachmentContent(
            tx,
            files,
            a,
            row.attachment_link_id,
            "original",
          )
        : null;
    const file = canonical?.object ?? (await files.get(row.object_key));
    if (!file || file.size > 10 * 1024 * 1024)
      fail("Source file unavailable or larger than 10 MB.");
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (hash(bytes) !== row.checksum_sha256) fail("Source checksum mismatch.");
    const input = {
      id: x.id,
      version: x.expectedVersion,
      name: row.original_name,
      type: row.media_type,
      checksum: hash(bytes),
    };
    const prior = await replayCommand<{ id: string; version: number }>(
      tx,
      a,
      key,
      "legal.document.upload",
      x.id,
      input,
    );
    if (prior) return prior;
    const result = await uploadDocument(
      tx,
      files,
      a,
      x.id,
      x.expectedVersion,
      row.original_name,
      row.media_type,
      bytes,
      key,
    );
    await tx
      .prepare(
        "INSERT INTO legal_file_preservations(item_id,source_type,source_id,object_key,checksum,created_by,created_at) VALUES(?,'attachment',?,?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(
        x.id,
        x.attachmentId,
        row.object_key,
        row.checksum_sha256,
        a.userId,
        Date.now(),
      )
      .run();
    return result;
  });
}
