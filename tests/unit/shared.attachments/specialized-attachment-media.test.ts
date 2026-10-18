import { describe, expect, it } from "vitest";
import { Document, Packer, Paragraph } from "docx";
import { attachmentPolicy, assertAttachmentMedia } from "../../../src/server/modules/files/public/attachments/policy";
import { detectedAttachmentMediaType } from "../../../src/server/modules/files/public/attachments/preview";

describe("specialized canonical attachment adapters", () => {
it("keeps CSV and DOCX as closed specialized media exceptions", async () => {
    const messaging = attachmentPolicy(
      "messaging_room",
      "message_attachment",
      "message",
    );
    const expense = attachmentPolicy("expense_claim", "receipt", "record");
    const legal = attachmentPolicy("legal_item", "legal_document", "record");
    const legalPortal = attachmentPolicy(
      "legal_item",
      "legal_portal_submission",
      "action",
    );
    expect(messaging.maxFiles).toBe(10);
    expect(() => assertAttachmentMedia(messaging, "text/csv", 8)).not.toThrow();
    expect(() => assertAttachmentMedia(expense, "text/csv", 8)).not.toThrow();

    const word = await Packer.toBuffer(
      new Document({ sections: [{ children: [new Paragraph("Evidence")] }] }),
    );
    const docx =
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    expect(detectedAttachmentMediaType(word, docx)).toBe(docx);
    expect(() => assertAttachmentMedia(legal, docx, word.byteLength)).not.toThrow();
    expect(legalPortal.portalUpload).toBe(true);
    expect(() =>
      assertAttachmentMedia(legalPortal, docx, word.byteLength),
    ).not.toThrow();
    expect(() => assertAttachmentMedia(legalPortal, "text/csv", 8)).toThrow();
    expect(
      detectedAttachmentMediaType(new Uint8Array([0x50, 0x4b, 3, 4]), docx),
    ).toBeNull();
    expect(() =>
      assertAttachmentMedia(messaging, docx, word.byteLength),
    ).toThrow();
  });
});
