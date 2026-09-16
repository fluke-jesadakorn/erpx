
import { workFetch } from "@/lib/work-fetch";
import { prepareDocumentPages } from "./document-pages";

export const receiptTypes = ["image/jpeg", "image/png", "image/webp", "application/pdf"];
export function validateReceipt(file: File, allowCsv = false) {
  if (!file.size || file.size > 10 * 1024 * 1024) throw Error("Choose a receipt up to 10 MB.");
  if (![...receiptTypes, ...(allowCsv ? ["text/csv"] : [])].includes(file.type))
    throw Error("Choose a JPG, PNG, WebP or PDF receipt.");
}
/** Retain each upload intent across retries, including a completed original whose pages failed. */
export function createReceiptUploader() {
  const attempts = new WeakMap<File, { url: string; original?: { id: string; checksum: string }; done?: boolean }>();
  return async (base: string, ownerId: string, file: File, payment = false, progress: (text: string) => void = () => {}) => {
    validateReceipt(file, !payment);
    let attempt = attempts.get(file);
    if (!attempt) {
      progress(`Uploading ${file.name}…`);
      const response = await workFetch(`/api/v1${base}/uploads`, {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ownerType: payment ? "expense_payment" : "expense_claim", ownerId,
          purpose: payment ? "payment_slip" : "merchant_receipt", filename: file.name, mediaType: file.type, bytes: file.size }),
      });
      const result = await response.json();
      if (!response.ok) throw Error(result.error?.message ?? "Unable to prepare upload.");
      attempt = { url: result.data.uploadUrl }; attempts.set(file, attempt);
    }
    if (!attempt.original) {
      const response = await workFetch(attempt.url, { method: "PUT", headers: { "Content-Type": file.type }, body: file });
      const result = await response.json();
      if (!response.ok) throw Error(result.error?.message ?? "Unable to upload receipt.");
      attempt.original = { id: result.data.id, checksum: result.data.checksum };
    }
    const original = attempt.original;
    if (!attempt.done) {
      try {
        await prepareDocumentPages(file, async (page, total, image) => {
          progress(`Preparing ${file.name} · page ${page} of ${total}…`);
          const response = await workFetch(`/api/v1${base}/attachments/${original.id}/derivatives?page=${page}&total=${total}&original=${original.checksum}`, {
            method: "POST", headers: { "Content-Type": "image/jpeg", "Idempotency-Key": `render-${original.id}-${page}` }, body: image,
          });
          const result = await response.json();
          if (!response.ok) throw Error(result.error?.message ?? "Unable to prepare this page.");
        });
        attempt.done = true;
      } catch (error) {
        throw Error(`The original receipt is saved. ${error instanceof Error ? error.message : "Unable to read this document."}`);
      }
    }
    return original;
  };
}
