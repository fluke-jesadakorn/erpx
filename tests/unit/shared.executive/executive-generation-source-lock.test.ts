import { describe, expect, it, vi } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { createExecutiveReportApplication } from "@erpx/platform-runtime-server/application/executive/reports";
import { createExecutiveGenerationApplication } from "@erpx/platform-runtime-server/application/executive/generation";
import { emptyDefinition } from "@erpx/platform-runtime-server/public/rules/canvas-domain";

const root = process.cwd();
const read = (path: string) => readFileSync(join(root, path), "utf8");

const sourceLock = "modules/platform-administration/server/src/adapters/executive-generation-source-lock.ts";
const sourceLockPublic = "modules/platform-administration/server/src/public/executive-generation-source-lock.ts";
const generation = "platform/runtime/server/src/adapters/executive/generation.ts";
const registry = "platform/runtime/server/src/adapters/native_command_sql.rs";
const platformRoute = "modules/platform-administration/server/src/adapters/platform_executive_http.rs";

describe("Platform executive generation-lock read is native", () => {
  it("deletes the last platform executive SELECT and its exports", () => {
    expect(existsSync(join(root, sourceLock))).toBe(false);
    expect(existsSync(join(root, sourceLockPublic))).toBe(false);
    expect(read("modules/platform-administration/server/package.json")).not.toContain("executive-generation-source-lock");
  });

  it("switches runtime generation to the native source-lock statement", () => {
    const source = read(generation);
    expect(source).not.toContain("executiveGenerationSourceLock");
    expect(source).toContain("nativeRuntimeStatement('runtime_stmt_23'");
    expect(source).toContain("{ companyId: company, now: Date.now(), actor: undefined }).bind(company)");
  });

  it("admits the retired read literal into the native statement registry", () => {
    const source = read(registry);
    expect(source).toContain('kind: "runtime_stmt_23"');
    expect(source).toContain(
      "SELECT company_id FROM engineering_verification_contexts WHERE company_id=$1 FOR SHARE",
    );
  });

  it("keeps the platform executive native route authority", () => {
    expect(read(platformRoute)).toContain('/api/v1/platform/executive');
  });
});

const actor = {
  holdingId: "holding-1", holdingSlug: "acme-co", holdingName: "Acme",
  companyId: "company-1", userId: "user-1", assignmentId: "assignment-1",
  role: "holding_owner",
  scope: { kind: "holding", holdingId: "holding-1", holdingSlug: "acme-co", holdingName: "Acme", companyId: "company-1" },
} as never;
const filters = { from: "2026-01-01", to: "2026-12-31", currency: "THB" };

describe("Executive report create is one scoped native statement", () => {
  it("pins the adapter and application to the atomic create kind", () => {
    const reports = read("platform/runtime/server/src/adapters/executive/reports.ts");
    expect(reports).toContain("nativeRuntimeStatement('executive_create_report', { companyId, now, actor: undefined })");
    expect(reports).not.toContain("'runtime_stmt_1'");
    expect(reports).not.toContain("'runtime_stmt_2'");
    const application = read("platform/runtime/server/src/application/executive/reports.ts");
    expect(application).toContain("repository.stageCreateReport(");
    expect(application).not.toContain("stageCreateReportEffect");
    const registry = read("platform/runtime/server/src/adapters/native_command_sql.rs");
    expect(registry).toContain('kind: "executive_create_report"');
  });

  it("commits exactly one create effect carrying the real company and clock", async () => {
    const stageCreateReport = vi.fn((...args: unknown[]) => ({ kind: "create", args }));
    const commit = vi.fn(async () => ({ id: "report-create-key" }));
    const app = createExecutiveReportApplication({
      repository: { stageCreateReport },
      clock: { now: () => 1_700_000_000_000 },
      identifiers: { next: () => "id-1" },
      commands: { commit, replay: vi.fn(async () => null) },
      requireAuthor: vi.fn(async () => {}),
      currentAuthority: vi.fn(async () => {}),
      canAuthor: vi.fn(async () => true),
      executiveEligible: vi.fn(async () => true),
      metricCatalogFor: vi.fn(async () => []),
    } as never);
    expect(await app.createReport(actor, "create-key", "Quarterly")).toEqual({ id: "report-create-key" });
    expect(stageCreateReport).toHaveBeenCalledTimes(1);
    expect(stageCreateReport).toHaveBeenCalledWith(
      "report-create-key", "company-1", "user-1", expect.any(String), expect.any(String), 1_700_000_000_000,
    );
    const calls = commit.mock.calls as unknown[][];
    expect(calls[0]?.[7]).toHaveLength(1);
    expect((calls[0]?.[7] as unknown[])[0]).toMatchObject({ kind: "create" });
  });
});

describe("Executive generation drain reads back the claimed lease", () => {
  it("pins the adapter to fetch the claimed job rather than returning null", () => {
    const source = read("platform/runtime/server/src/adapters/executive/generation.ts");
    expect(source).toContain(".bind(token, value, now).first<Job>()");
    expect(source).not.toContain("return null; /* RETURNING-row projection");
  });

  it("invokes the AI provider once the native lease is read back", async () => {
    const token = "lease-token";
    const job = {
      id: "job-1", report_id: "report-1", company_id: "company-1", user_id: "user-1",
      assignment_id: "assignment-1", scope_key: "holding", question: "Add a chart",
      input_json: JSON.stringify({ definition: emptyDefinition("Quarterly"), filters, version: 1, selectedWidget: null }),
      actor_json: JSON.stringify(actor), provider_json: "{}", state: "running",
      proposal_json: null, error: null, created_at: 1, lease_token: token,
    };
    const generate = vi.fn(async () => ({
      result: { message: "Done", definition: emptyDefinition("Quarterly"), followUps: [] },
      inputTokens: 10, outputTokens: 20,
    }));
    const commit = vi.fn(async () => ({}));
    const app = createExecutiveGenerationApplication({
      repository: {
        source10: vi.fn(async () => ({ success: true })),
        source11: vi.fn(async () => job),
        source13: vi.fn(() => "ready"),
        source14: vi.fn(async () => "cleared"),
        source15: vi.fn(async () => {}),
        source16: vi.fn(async () => "cleared"),
        guard13: vi.fn(() => "guard"),
      },
      clock: { now: () => 2 },
      identifiers: { next: () => token },
      commands: { commit, replay: vi.fn(async () => null) },
      batch: vi.fn(async () => []),
      sha256: vi.fn(async () => "session"),
      schemaRejected: vi.fn(),
      generate,
      withAiGatewayScope: vi.fn((endpoint: string) => endpoint),
      resolveAiProvider: vi.fn(async () => ({ snapshot: {}, endpoint: "https://ai.invalid" })),
      generationFeatureGuards: vi.fn(() => []),
      aiRequestQuotaLock: vi.fn(() => "quota"),
      requireAuthor: vi.fn(async () => {}),
      reportAccess: vi.fn(async () => ({ canEdit: true, r: { version: 1 }, filters, definition: emptyDefinition("Quarterly") })),
      executiveAiContext: vi.fn(async () => ({ catalog: [] })),
      assertProductionModuleEnabled: vi.fn(async () => {}),
      aiSnapshotGuard: vi.fn(() => "guard"),
      aiUsageStatement: vi.fn(() => "usage"),
    } as never);
    expect(await app.drainExecutiveGeneration()).toBe(true);
    expect(generate).toHaveBeenCalledTimes(1);
    expect((commit.mock.calls as unknown[][])[0]?.[2]).toBe("executive.generation.complete");
  });
});
