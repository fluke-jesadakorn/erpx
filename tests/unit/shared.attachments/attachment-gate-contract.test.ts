import { describe, expect, it } from "vitest";
import {
  ATTACHMENT_DRAFT_MS,
  ATTACHMENT_MAX_BYTES,
  ATTACHMENT_RESERVATION_MS,
  attachmentUploadsFrozen,
  attachmentsV2Enabled,
  attachmentsV2ReadOnly,
  attachmentsV2Readable,
  requireAttachmentsV2,
  requireAttachmentsV2Read,
} from "@erpx/server-files/public/attachments/feature";

function environment(values: Record<string, string | undefined> = {}): NodeJS.ProcessEnv {
  return { NODE_ENV: 'test', ...values };
}

describe("attachment global gate contract", () => {
  it("opens only for the exact enabled value", () => {
    for (const value of [undefined, "", "false", "TRUE", "1", "yes"])
      expect(attachmentsV2Enabled(environment({ ERPX_ATTACHMENTS_V2: value }))).toBe(false);

    expect(attachmentsV2Enabled(environment({ ERPX_ATTACHMENTS_V2: "true" }))).toBe(true);
  });

  it("fails both canonical reads and writes closed while the gate is off", () => {
    const off = environment({ ERPX_ATTACHMENTS_V2: "false" });

    expect(attachmentsV2Readable(off)).toBe(false);
    expect(() => requireAttachmentsV2(off)).toThrow(
      expect.objectContaining({ status: 404, code: "ATTACHMENTS_DISABLED" }),
    );
    expect(() => requireAttachmentsV2Read(off)).toThrow(
      expect.objectContaining({ status: 404, code: "ATTACHMENTS_DISABLED" }),
    );
  });

  it("keeps rollback read-only: bound reads remain possible but canonical writes stay closed", () => {
    const rollback = environment({
      ERPX_ATTACHMENTS_V2: "false",
      ERPX_ATTACHMENTS_READ_ONLY: "true",
    });

    expect(attachmentsV2ReadOnly(rollback)).toBe(true);
    expect(attachmentsV2Readable(rollback)).toBe(true);
    expect(() => requireAttachmentsV2Read(rollback)).not.toThrow();
    expect(() => requireAttachmentsV2(rollback)).toThrow(
      expect.objectContaining({ status: 404, code: "ATTACHMENTS_DISABLED" }),
    );
  });

  it("does not turn an enabled release into rollback mode", () => {
    const enabled = environment({
      ERPX_ATTACHMENTS_V2: "true",
      ERPX_ATTACHMENTS_READ_ONLY: "true",
    });

    expect(attachmentsV2ReadOnly(enabled)).toBe(false);
    expect(attachmentsV2Readable(enabled)).toBe(true);
    expect(() => requireAttachmentsV2(enabled)).not.toThrow();
    expect(() => requireAttachmentsV2Read(enabled)).not.toThrow();
  });

  it("freezes new intake only for the exact freeze value and pins lifecycle boundaries", () => {
    expect(attachmentUploadsFrozen(environment())).toBe(false);
    expect(attachmentUploadsFrozen(environment({ ERPX_ATTACHMENT_UPLOAD_FREEZE: "false" }))).toBe(false);
    expect(attachmentUploadsFrozen(environment({ ERPX_ATTACHMENT_UPLOAD_FREEZE: "TRUE" }))).toBe(false);
    expect(attachmentUploadsFrozen(environment({ ERPX_ATTACHMENT_UPLOAD_FREEZE: "true" }))).toBe(true);
    expect(ATTACHMENT_MAX_BYTES).toBe(10 * 1024 * 1024);
    expect(ATTACHMENT_RESERVATION_MS).toBe(15 * 60_000);
    expect(ATTACHMENT_DRAFT_MS).toBe(7 * 24 * 60 * 60_000);
  });
});
