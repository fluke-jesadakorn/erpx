import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  NATIVE_FEEDBACK_ENV,
  NATIVE_INVITATIONS_ENV,
  feedbackConfig,
  invitationsConfig,
  nativeBetaFeedback,
  nativeBetaInvitations,
  parseFeedbackResponse,
  parseInvitationsResponse,
  validFeedbackCompany,
  type FeedbackPost,
} from "../../../packages/server-manage_erpx/src/adapters/native-lane-34/client";

const root = resolve(process.cwd());
const read = (path: string) => readFileSync(resolve(root, path), "utf8");
const hash = (value: string | object) =>
  createHash("sha256").update(typeof value === "string" ? value : JSON.stringify(value)).digest("hex");

const SQL = "SELECT id,body,created_at FROM beta_feedback WHERE company_id=? ORDER BY created_at DESC LIMIT 100";
const SQL_HASH = "4c06c88da1ff78ab57ca3db2448a9e767687c9c23b26c9f3f7aa46b2d6743640";
const ROWS = [{ id: "fb-1", body: "hello", created_at: 1700000000000 }];
const RESULT_HASH = "bec9253bbc72127016fb0f4008d81b3b5958ca234e6b6b2f0f31ffb3737a0f01";
const BODY = { feedback: ROWS };

const INV_SQL = "SELECT i.id,i.email,i.branch_id,i.expires_at,i.accepted_at,i.revoked_at,CASE WHEN i.revoked_at IS NOT NULL THEN 'Revoked' WHEN i.accepted_at IS NOT NULL THEN 'Joined' WHEN i.expires_at<=(floor(extract(epoch FROM clock_timestamp()))::bigint*1000) THEN 'Expired' ELSE 'Invited' END invitation_state FROM beta_team_invitations i JOIN organization_nodes b ON b.id=i.branch_id WHERE b.company_id=? ORDER BY i.created_at DESC LIMIT 100";
const INV_SQL_HASH = "65f6195fead59aefeb34640311f8608409c672b9ead4be7b5e4b8bb558288b30";
const INV_ROWS = [{
  id: "inv-1",
  email: "owner@example.test",
  branch_id: "branch-1",
  expires_at: 1700000000000,
  accepted_at: null,
  revoked_at: null,
  invitation_state: "Invited",
}];
const INV_RESULT_HASH = "10fee023515cf547c7ca5891df2f014533821f0d427d511d13f52658b29accd7";
const INV_BODY = { invitations: INV_ROWS };

const invConfig = (extra: Record<string, unknown> = {}) => JSON.stringify({
  privateBaseUrl: "http://127.0.0.1:3301",
  timeoutMs: 1000,
  holdings: [
    { origin: "https://acme.example.test", host: "acme.example.test", holdingSlug: "acme" },
    { origin: "https://northstar.example.test", host: "northstar.example.test", holdingSlug: "northstar" },
  ],
  ...extra,
});

const config = (extra: Record<string, unknown> = {}) => JSON.stringify({
  privateBaseUrl: "http://127.0.0.1:3301",
  timeoutMs: 1000,
  holdings: [{ origin: "https://acme.example.test", holdingSlug: "acme" }],
  ...extra,
});

function post(value: unknown = BODY, status = 200) {
  const calls: { url: string; headers: Record<string, string> }[] = [];
  const fn: FeedbackPost = async (url, headers) => {
    calls.push({ url: url.toString(), headers });
    return { status, value };
  };
  return { fn, calls };
}

const saved = process.env[NATIVE_FEEDBACK_ENV];
const savedInvitations = process.env[NATIVE_INVITATIONS_ENV];
afterEach(() => {
  if (saved === undefined) delete process.env[NATIVE_FEEDBACK_ENV];
  else process.env[NATIVE_FEEDBACK_ENV] = saved;
  if (savedInvitations === undefined) delete process.env[NATIVE_INVITATIONS_ENV];
  else process.env[NATIVE_INVITATIONS_ENV] = savedInvitations;
});

describe("native manage_erpx lane 34 beta feedback read", () => {
  it("pins the rendered SQL and result oracles", () => {
    expect(hash(SQL)).toBe(SQL_HASH);
    expect(hash(ROWS)).toBe(RESULT_HASH);
    expect(SQL).toMatch(/^SELECT /);
    expect(SQL.match(/\?/g)).toHaveLength(1);
    expect(SQL).not.toMatch(/FOR UPDATE|LOCK |;|INSERT |UPDATE |DELETE /);
  });

  it("accepts only closed loopback/front-origin configuration and fails closed otherwise", () => {
    expect(feedbackConfig(config())).toEqual({
      base: "http://127.0.0.1:3301",
      timeoutMs: 1000,
      holding: { origin: "https://acme.example.test", holdingSlug: "acme" },
    });
    const rejects = (raw: string | undefined, code: string) => {
      try {
        feedbackConfig(raw);
      } catch (error) {
        expect(error).toMatchObject({ status: 503, code });
        return;
      }
      throw new Error(`expected rejection: ${code}`);
    };
    rejects(undefined, "MANAGE_ERPX_NATIVE_FEEDBACK_UNAVAILABLE");
    rejects("{", "MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
    rejects(config({ privateBaseUrl: "http://10.0.0.8:3301" }), "MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
    rejects(config({ privateBaseUrl: "http://127.0.0.1:3100" }), "MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
    rejects(config({ timeoutMs: 249 }), "MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
    rejects(config({ holdings: [] }), "MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
    rejects(config({ holdings: [{ origin: "https://acme.example.test", holdingSlug: "Acme" }] }), "MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
    rejects(config({ holdings: [{ origin: "http://acme.example.test", holdingSlug: "acme" }] }), "MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
  });

  it("validates the exact response shape and fails closed on any deviation", () => {
    expect(parseFeedbackResponse(BODY)).toEqual(ROWS);
    expect(validFeedbackCompany("acme:co")).toBe(true);
    expect(validFeedbackCompany("bad/id?")).toBe(false);
    for (const value of [
      {},
      { feedback: {}, },
      { feedback: [...ROWS, { ...ROWS[0], extra: 1 }] },
      { feedback: [{ ...ROWS[0], id: "" }] },
      { feedback: [{ ...ROWS[0], body: "x".repeat(3001) }] },
      { feedback: [{ ...ROWS[0], created_at: 1.5 }] },
      { feedback: [{ ...ROWS[0], created_at: -1 }] },
      { feedback: Array.from({ length: 101 }, () => ROWS[0]) },
    ]) {
      try {
        parseFeedbackResponse(value);
      } catch (error) {
        expect(error).toMatchObject({ status: 503, code: "MANAGE_ERPX_NATIVE_FEEDBACK_RESPONSE_INVALID" });
        continue;
      }
      throw new Error("expected strict response rejection");
    }
  });

  it("transports Company scope only and returns the native projection", async () => {
    process.env[NATIVE_FEEDBACK_ENV] = config();
    const { fn, calls } = post();
    await expect(nativeBetaFeedback("acme:co", fn)).resolves.toEqual(ROWS);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url).toBe("http://127.0.0.1:3301/api/v1/holdings/acme/manage-erpx/beta-feedback");
    expect(calls[0]!.headers).toMatchObject({
      host: "acme.example.test",
      origin: "https://acme.example.test",
      "x-erpx-context-company": "acme:co",
    });
  });

  it("fails closed without configuration, on transport failure and on a bad response", async () => {
    delete process.env[NATIVE_FEEDBACK_ENV];
    await expect(nativeBetaFeedback("acme:co", post().fn))
      .rejects.toMatchObject({ status: 503, code: "MANAGE_ERPX_NATIVE_FEEDBACK_UNAVAILABLE" });

    process.env[NATIVE_FEEDBACK_ENV] = config();
    const boom: FeedbackPost = async () => { throw new Error("offline"); };
    await expect(nativeBetaFeedback("acme:co", boom))
      .rejects.toMatchObject({ status: 503, code: "MANAGE_ERPX_NATIVE_FEEDBACK_UNAVAILABLE" });
    await expect(nativeBetaFeedback("acme:co", post({ feedback: "wrong" }).fn))
      .rejects.toMatchObject({ status: 503, code: "MANAGE_ERPX_NATIVE_FEEDBACK_RESPONSE_INVALID" });
    await expect(nativeBetaFeedback("acme:co", post({ error: { code: "BOOM", message: "no" } }, 503).fn))
      .rejects.toMatchObject({ status: 503, code: "MANAGE_ERPX_NATIVE_FEEDBACK_UNAVAILABLE" });
    await expect(nativeBetaFeedback("bad/id?", post().fn))
      .rejects.toMatchObject({ status: 422, code: "MANAGE_ERPX_NATIVE_FEEDBACK_REQUEST_INVALID" });
  });

  it("pins the owner Rust SQL, router and the deleted TypeScript literal", () => {
    const domain = read("packages/server-manage_erpx/src/adapters/native_lane_34/domain.rs");
    const postgres = read("packages/server-manage_erpx/src/adapters/native_lane_34/postgres.rs");
    const http = read("packages/server-manage_erpx/src/adapters/native_lane_34/http.rs");
    const api = read("packages/server-manage_erpx/src/adapters/beta-api.ts");
    const client = read("packages/server-manage_erpx/src/adapters/native-lane-34/client.ts");
    expect(domain).toContain(`pub const FEEDBACK_SQL: &str = "${SQL}";`);
    expect(postgres).toContain("sqlx::query(FEEDBACK_SQL)");
    expect(postgres).toContain(".bind(company.as_str())");
    expect(postgres).toContain(".fetch_all(&mut *connection)");
    expect(http).toContain("pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError>");
    expect(http).toContain('"/api/v1/holdings/{holding_slug}/manage-erpx/beta-feedback"');
    expect(http).toContain('HeaderName::from_static("x-erpx-context-company")');
    expect(api).toContain("async feedback(companyId:string){return nativeBetaFeedback(companyId);}");
    expect(api).not.toContain(SQL);
    expect(api).not.toContain("beta_feedback WHERE company_id=?");
    expect(client).not.toContain("SELECT ");
    expect(client).toContain('"MANAGE_ERPX_NATIVE_FEEDBACK_UNAVAILABLE"');
    expect(client).toContain('status = 503');
    expect(client).toContain("throw new ApiError(status, code, message)");
    expect(domain).toContain("pub const INVITATIONS_SQL: &str = concat!(");
    expect(postgres).toContain("sqlx::query(INVITATIONS_SQL)");
    expect(http).toContain('"/api/v1/holdings/{holding_slug}/manage-erpx/beta-invitations"');
    expect(http).toContain("get(invitations)");
    expect(api).toContain("async invitations(companyId:string,holdingSlug:string){return nativeBetaInvitations(companyId,holdingSlug);}");
    expect(api).not.toContain(INV_SQL);
    expect(api).not.toContain("invitation_state FROM beta_team_invitations");
    expect(client).not.toContain("SELECT ");
  });

  it("pins the owner TypeScript SELECT KPI at 17 (before 18)", () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".ts")) total += (readFileSync(path, "utf8").match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(root, "packages/server-manage_erpx/src"));
    expect(total).toBe(17);
  });
});

describe("native manage_erpx lane 34 multi-holding invitations read", () => {
  it("pins the rendered invitations SQL and result oracles", () => {
    expect(hash(INV_SQL)).toBe(INV_SQL_HASH);
    expect(hash(INV_ROWS)).toBe(INV_RESULT_HASH);
    expect(INV_SQL).toMatch(/^SELECT /);
    expect(INV_SQL.match(/\?/g)).toHaveLength(1);
    expect(INV_SQL).toContain("JOIN organization_nodes b ON b.id=i.branch_id WHERE b.company_id=?");
    expect(INV_SQL).not.toMatch(/FOR UPDATE|LOCK |;|INSERT |UPDATE |DELETE /);
  });

  it("accepts exact distinct registered Holdings and fails closed on drift", () => {
    expect(invitationsConfig(invConfig()).holdings.map((h) => h.holdingSlug)).toEqual(["acme", "northstar"]);
    const rejects = (raw: string | undefined, code: string) => {
      try {
        invitationsConfig(raw);
      } catch (error) {
        expect(error).toMatchObject({ status: 503, code });
        return;
      }
      throw new Error(`expected rejection: ${code}`);
    };
    rejects(undefined, "MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_REQUIRED");
    rejects("{", "MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
    rejects(invConfig({ privateBaseUrl: "http://10.0.0.8:3301" }), "MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
    rejects(invConfig({ privateBaseUrl: "http://127.0.0.1:3100" }), "MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
    rejects(invConfig({ timeoutMs: 249 }), "MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
    rejects(invConfig({ holdings: [] }), "MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
    rejects(invConfig({ holdings: [{ origin: "https://acme.example.test", host: "acme.example.test", holdingSlug: "acme" }, { origin: "https://acme.example.test", host: "acme.example.test", holdingSlug: "acme" }] }), "MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
    rejects(invConfig({ holdings: [{ origin: "https://acme.example.test", host: "wrong.example.test", holdingSlug: "acme" }] }), "MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
  });

  it("validates the invitation envelope strictly and fails closed on deviation", () => {
    expect(parseInvitationsResponse(INV_BODY)).toEqual(INV_ROWS);
    for (const value of [
      {},
      { invitations: {} },
      { invitations: [{ ...INV_ROWS[0], extra: 1 }] },
      { invitations: [{ ...INV_ROWS[0], invitation_state: "Pending" }] },
      { invitations: [{ ...INV_ROWS[0], expires_at: -1 }] },
      { invitations: [{ ...INV_ROWS[0], accepted_at: 1.5 }] },
      { invitations: Array.from({ length: 101 }, () => INV_ROWS[0]) },
    ]) {
      try {
        parseInvitationsResponse(value);
      } catch (error) {
        expect(error).toMatchObject({ status: 503, code: "MANAGE_ERPX_NATIVE_INVITATIONS_RESPONSE_INVALID" });
        continue;
      }
      throw new Error("expected strict invitation rejection");
    }
  });

  it("routes each Holding to its own host with the Company bind", async () => {
    process.env[NATIVE_INVITATIONS_ENV] = invConfig();
    const { fn, calls } = post(INV_BODY);
    await expect(nativeBetaInvitations("acme:co", "northstar", fn)).resolves.toEqual(INV_ROWS);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url).toBe("http://127.0.0.1:3301/api/v1/holdings/northstar/manage-erpx/beta-invitations");
    expect(calls[0]!.headers).toMatchObject({
      host: "northstar.example.test",
      origin: "https://northstar.example.test",
      "x-erpx-context-company": "acme:co",
    });
  });

  it("fails closed on unregistered Holding, transport failure and bad response", async () => {
    delete process.env[NATIVE_INVITATIONS_ENV];
    await expect(nativeBetaInvitations("acme:co", "acme", post(INV_BODY).fn))
      .rejects.toMatchObject({ status: 503, code: "MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_REQUIRED" });
    process.env[NATIVE_INVITATIONS_ENV] = invConfig();
    await expect(nativeBetaInvitations("acme:co", "unknown", post(INV_BODY).fn))
      .rejects.toMatchObject({ status: 503, code: "MANAGE_ERPX_NATIVE_INVITATIONS_TARGET_UNAVAILABLE" });
    const boom: FeedbackPost = async () => { throw new Error("offline"); };
    await expect(nativeBetaInvitations("acme:co", "acme", boom))
      .rejects.toMatchObject({ status: 503, code: "MANAGE_ERPX_NATIVE_INVITATIONS_UNAVAILABLE" });
    await expect(nativeBetaInvitations("acme:co", "acme", post({ invitations: "wrong" }).fn))
      .rejects.toMatchObject({ status: 503, code: "MANAGE_ERPX_NATIVE_INVITATIONS_RESPONSE_INVALID" });
    await expect(nativeBetaInvitations("bad/id?", "acme", post(INV_BODY).fn))
      .rejects.toMatchObject({ status: 422, code: "MANAGE_ERPX_NATIVE_INVITATIONS_REQUEST_INVALID" });
  });
});
