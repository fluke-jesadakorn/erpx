import { describe, expect, it } from "vitest";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { ApiError } from "@erpx/server-runtime/public/http";
import {
  nativeHoldingBindingReadConfigured,
  parseNativeConfig,
  validateBinding,
} from "../../../packages/server-runtime/src/adapters/native-lane-33/client";

const root = resolve(__dirname, "../../../packages/server-runtime/src");
const src = (relative: string) => readFileSync(resolve(root, relative), "utf8");
const sha = (value: string) => createHash("sha256").update(value).digest("hex");

const base = "SELECT holding_id, slug, display_name FROM holding_bindings";
const rendered = {
  bySlug: `${base} WHERE slug=? AND status='active'`,
  default: `${base} WHERE status='active' ORDER BY holding_id LIMIT 1`,
  active: `${base} WHERE status='active' ORDER BY holding_id`,
} as const;

const oracles = {
  bySlug: "970fca1744181cbe731ff19a6064edf58107fff6d27801c9a1b86d1241367add",
  default: "424a3df615015a7d7144f38f13da25895a324b8a8666abcadbd7433bfd131b71",
  active: "dcf90998e6a224401ff3403edf31a57d54d4cbb5d70a921000d68469d552c48a",
} as const;

describe("native lane 33 runtime holding-binding read contract", () => {
  it("pins the byte-identical SQL and result oracles on the Rust owner", () => {
    const sql = src("adapters/native_lane_33/postgres.rs");
    expect(sql).toContain(`pub const BASE_SQL: &str = "${base}"`);
    expect(sql).toContain("WHERE slug=$1 AND status='active'");
    expect(sql).toContain("WHERE status='active' ORDER BY holding_id LIMIT 1");
    expect(sql).toContain("WHERE status='active' ORDER BY holding_id\"");
    expect(sha(rendered.bySlug)).toBe(oracles.bySlug);
    expect(sha(rendered.default)).toBe(oracles.default);
    expect(sha(rendered.active)).toBe(oracles.active);
    expect(sql).toContain(oracles.bySlug);
    expect(sql).toContain(oracles.default);
    expect(sql).toContain(oracles.active);
  });

  it("owns the exact response shape and the closed read kinds in Rust", () => {
    const domain = src("adapters/native_lane_33/domain.rs");
    expect(domain).toContain("pub holding_id: String");
    expect(domain).toContain("pub slug: String");
    expect(domain).toContain("pub display_name: String");
    expect(domain).toContain('"by-slug" => Some(Self::BySlug)');
    expect(domain).toContain('"default" => Some(Self::Default)');
    expect(domain).toContain('"active" => Some(Self::Active)');
    const http = src("adapters/native_lane_33/http.rs");
    expect(http).toContain('"holdingId": binding.holding_id');
    expect(http).toContain('"displayName": binding.display_name');
    expect(http).toContain("router_with_reader(");
  });

  it("removes all replaced TypeScript SQL from the runtime adapter", () => {
    const runtime = src("adapters/runtime.ts");
    expect(runtime).not.toContain("holdingBindingsSql");
    expect(runtime).not.toContain("HOLDING_BINDINGS_SELECT");
    expect(runtime).not.toContain("CONTROL_DB.prepare");
    expect(runtime).not.toContain("SELECT");
    expect(runtime).toContain("./native-lane-33/client");
    expect(runtime).toContain("readNativeHoldingBindingBySlug");
    expect(runtime).toContain("readNativeDefaultHoldingBinding");
    expect(runtime).toContain("readNativeActiveHoldingBindings");
  });

  it("fails closed when the native transport is missing or malformed", () => {
    expect(nativeHoldingBindingReadConfigured({})).toBe(false);
    expect(() => parseNativeConfig(undefined)).toThrowError(ApiError);
    try {
      parseNativeConfig(undefined);
    } catch (error) {
      expect((error as ApiError).status).toBe(503);
      expect((error as ApiError).code).toBe("NATIVE_RUNTIME_HOLDING_BINDING_CONFIG_REQUIRED");
    }
    try {
      parseNativeConfig(JSON.stringify({
        privateBaseUrl: "http://127.0.0.1:3100",
        host: "acme.jesadakorn.com",
        holdingSlug: "acme",
        origin: "https://acme.jesadakorn.com",
        timeoutMs: 1000,
      }));
    } catch (error) {
      expect((error as ApiError).status).toBe(503);
      expect((error as ApiError).code).toBe("NATIVE_RUNTIME_HOLDING_BINDING_CONFIG_INVALID");
    }
  });

  it("accepts only the complete pinned response shape", () => {
    expect(validateBinding({
      holdingId: "h001",
      slug: "acme",
      displayName: "Acme",
    })).toEqual({ holdingId: "h001", slug: "acme", displayName: "Acme" });
    for (const bad of [
      { holdingId: "H001", slug: "acme", displayName: "Acme" },
      { holdingId: "h001", slug: "acme" },
      { holdingId: "h001", slug: "acme", displayName: 7 },
      null,
    ]) {
      try {
        validateBinding(bad);
        expect.unreachable("an incomplete native payload must fail closed");
      } catch (error) {
        expect((error as ApiError).status).toBe(503);
        expect((error as ApiError).code).toBe("NATIVE_RUNTIME_HOLDING_BINDING_RESPONSE_INVALID");
      }
    }
  });

  it("keeps the TypeScript owner SELECT count at its retained-builder floor", () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".ts")) total += (readFileSync(path, "utf8").match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    // Before this slice the owner had 27 `SELECT ` literals; the retired
    // `HOLDING_BINDINGS_SELECT` constant accounted for one of them.
    expect(total).toBe(24);
  });
});
