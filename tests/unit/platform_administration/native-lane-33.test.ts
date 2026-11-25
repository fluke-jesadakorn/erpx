import { describe, expect, it } from "vitest";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { ApiError } from "@erpx/server-runtime/public/http";
import {
  nativeReleaseChildImageConfigured,
  parseNativeConfig,
  selectHolding,
  validateImage,
} from "../../../packages/server-platform_administration/src/adapters/native-lane-33/client";

const root = resolve(__dirname, "../../../packages/server-platform_administration/src");
const src = (relative: string) => readFileSync(resolve(root, relative), "utf8");
const sha = (value: string) => createHash("sha256").update(value).digest("hex");

const cfg = {
  privateBaseUrl: "http://127.0.0.1:4019",
  timeoutMs: 1000,
  holdings: [
    {
      origin: "https://acme.jesadakorn.com",
      host: "acme.jesadakorn.com",
      holdingId: "h001",
      holdingSlug: "acme",
    },
    {
      origin: "https://northstar.jesadakorn.com",
      host: "northstar.jesadakorn.com",
      holdingId: "h002",
      holdingSlug: "northstar",
    },
  ],
};

describe("native lane 33 platform release child-image contract", () => {
  it("pins the byte-identical SQL and result oracle on the Rust owner", () => {
    const sql = src("adapters/native_lane_33/postgres.rs");
    const oracle = "0ebb5e0ec16152ee113377a86f89d1b62045c5a9a985f8e8fc66bbb7de561f81";
    expect(src("adapters/native_lane_33/domain.rs")).toContain(oracle);
    expect(sql).toContain("WHERE acceptance.id=$1 AND acceptance.company_id=$2");
    expect(sql).toContain("jsonb_agg(to_jsonb(review) ORDER BY review.criterion_id)");
    expect(sql).toContain("jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id)");
    expect(sql).toContain("to_jsonb(revision)");
    const rendered = `SELECT jsonb_build_object('acceptance',to_jsonb(acceptance),
 'reviews',(SELECT jsonb_agg(to_jsonb(review) ORDER BY review.criterion_id) FROM company_release_gate_reviews review WHERE review.release_id=acceptance.id AND review.company_id=acceptance.company_id),
 'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM company_release_events event WHERE event.release_id=acceptance.id AND event.company_id=acceptance.company_id),
 'revision',(SELECT to_jsonb(revision) FROM company_release_revisions revision WHERE revision.company_id=acceptance.company_id))::text image FROM company_release_acceptances acceptance WHERE acceptance.id=? AND acceptance.company_id=?`;
    expect(sha(rendered)).toBe(oracle);
  });

  it("owns the bounded response shape and company/release headers", () => {
    const http = src("adapters/native_lane_33/http.rs");
    expect(http).toContain('"image": image.map(|image| image.image)');
    expect(http).toContain("x-erpx-context-company");
    expect(http).toContain("x-erpx-context-release");
    expect(http).toContain("router_for_holdings");
    expect(http).toContain("StatusCode::SERVICE_UNAVAILABLE");
  });

  it("removes the replaced TypeScript SQL completely", () => {
    const text = src("adapters/company-release-attachment-read-sources.ts");
    expect(text).not.toContain("SELECT ");
    expect(text).not.toContain("jsonb_build_object(");
    expect(text).not.toContain("releaseChildAggSql");
    expect(text).toContain("readNativeReleaseChildImage(actor,id)");
  });

  it("selects a registered Holding per actor and fails closed otherwise", () => {
    const parsed = parseNativeConfig(JSON.stringify(cfg));
    expect(parsed.holdings).toHaveLength(2);
    expect(selectHolding(parsed, {
      holdingId: "h002",
      holdingSlug: "northstar",
      companyId: "company-001",
    }).host).toBe("northstar.jesadakorn.com");
    try {
      selectHolding(parsed, {
        holdingId: "h003",
        holdingSlug: "acme",
        companyId: "company-001",
      });
      expect.unreachable("an unregistered Holding must fail closed");
    } catch (error) {
      expect((error as ApiError).status).toBe(503);
      expect((error as ApiError).code).toBe("PLATFORM_RELEASE_IMAGE_TARGET_INVALID");
    }
    expect(nativeReleaseChildImageConfigured({})).toBe(false);
  });

  it("accepts only the owner-built opaque image string", () => {
    expect(validateImage('{"acceptance":{}}')).toBe('{"acceptance":{}}');
    expect(validateImage(null)).toBeNull();
    for (const bad of ["[]", "SELECT 1", 7, "x".repeat(1_048_577)]) {
      try {
        validateImage(bad);
        expect.unreachable("an incomplete native image must fail closed");
      } catch (error) {
        expect((error as ApiError).status).toBe(503);
      }
    }
  });
});
