import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ATTACHMENT_MAX_BYTES,
  attachmentApiBase,
  attachmentMediaType,
  formatAttachmentBytes,
  publicMediaPublicationUrl,
  uploadAttachmentBytes,
  validateGenericAttachment,
} from "@erpx/feature-files/lib/attachment-client";

afterEach(()=>vi.unstubAllGlobals());

describe("attachment client policy", () => {
  it("maps holding workspace paths to their authenticated API scope", () => {
    expect(attachmentApiBase("/holdings/acme")).toBe("/api/v1/holdings/acme");
    expect(attachmentApiBase("/holdings/acme/branches/bkk/")).toBe(
      "/api/v1/holdings/acme/branches/bkk",
    );
    expect(attachmentApiBase("/api/v1/holdings/acme")).toBe("/api/v1/holdings/acme");
  });

  it("accepts only generic image and PDF formats within ten MiB", () => {
    expect(validateGenericAttachment({ name: "receipt.heic", type: "", size: 1234 })).toBeNull();
    expect(attachmentMediaType({ name: "receipt.heic", type: "" })).toBe("image/heic");
    expect(validateGenericAttachment({ name: "scan.pdf", type: "application/pdf", size: ATTACHMENT_MAX_BYTES })).toBeNull();
    expect(validateGenericAttachment({ name: "sheet.csv", type: "text/csv", size: 1234 })).toContain("JPEG");
    expect(validateGenericAttachment({ name: "large.png", type: "image/png", size: ATTACHMENT_MAX_BYTES + 1 })).toContain("10 MB");
    expect(validateGenericAttachment({ name: "empty.pdf", type: "application/pdf", size: 0 })).toContain("empty");
  });

  it("formats file sizes for accessible status text", () => {
    expect(formatAttachmentBytes(12)).toBe("12 B");
    expect(formatAttachmentBytes(2048)).toBe("2 KB");
    expect(formatAttachmentBytes(5 * 1024 * 1024)).toBe("5.0 MB");
  });

  it("keeps approved derivatives on the explicit public publication route", () => {
    expect(publicMediaPublicationUrl("/holdings/acme", "publication-1")).toBe(
      "/api/public/holdings/acme/media-publications/publication-1/content",
    );
    expect(() => publicMediaPublicationUrl("/customer-portal/acme", "publication-1")).toThrow(
      "holding workspace",
    );
  });

  it("preserves the explicit scan-pending acceptance status from canonical PUT",async()=>{
    vi.stubGlobal("fetch",vi.fn().mockResolvedValue(Response.json({data:{uploadIntentId:"upload-1",attachmentLinkId:"link-1",sourceId:"source-1",state:"quarantined",status:"scan_pending"}},{status:202})));
    const result=await uploadAttachmentBytes("/api/v1/holdings/acme/uploads/upload-1/content",new File([new Uint8Array([1])],"evidence.png",{type:"image/png"}),"image/png");
    expect(result).toEqual({uploadIntentId:"upload-1",attachmentLinkId:"link-1",sourceId:"source-1",state:"quarantined",status:"scan_pending"});
  });
});
