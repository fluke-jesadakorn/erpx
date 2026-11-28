import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, test, vi } from "vitest";

/**
 * G4 / retire.brand.attachment-facts — hermetic verification of the Brand
 * private-attachment read. The probe predicate is owned by Identity Access
 * (`currentAssignmentReadScope`); the fixed scope envelope now lives in the
 * Rust-owned `attachment-scope-head-sources.json` contract, bound with
 * `include_str!`, so no owner TypeScript statement token remains. This suite
 * proves the live Files read path still executes the exact recorded probe with
 * unchanged binds, that the image/source gates still fire, and that the Files
 * publication consumers are already native-only (8d71a830a).
 */
const h = vi.hoisted(() => ({
  image: "before",
  source: {
    setId: "set-1", targetType: "branch_brand", targetId: "branch-1",
    purposeKey: "primary", componentKind: "publication" as const, componentRef: null, witness: "w1",
  },
  statements: [] as { sql: string; values: unknown[] }[],
  reads: 0,
  changeImage: false,
}));

vi.mock("server-only", () => ({}));
vi.mock("@erpx/commercial-server/public/purchase-command-boundary", () => ({ companyPurchaseReadLocks: () => [] }));
vi.mock("@erpx/commercial-server/public/package-locks", () => ({ lockPackageSettlementSources: async () => undefined }));
vi.mock("@erpx/identity-access-server/public/queued-command-authority", () => ({ lockQueuedCommandAuthority: async () => undefined }));
vi.mock("@erpx/pms-server/public/scheduled-feature-source-locks", () => ({ lockScheduledFeatureSources: async () => undefined }));
vi.mock("@erpx/finance-server/public/finance-transactions", () => ({ flushFinanceDeferredEffects: async () => undefined }));
vi.mock("@erpx/identity-access-server/public/private-file-denial-authority", () => ({
  privateFileDenialAuthoritySourceLocks: () => [],
  privateFileDenialAuthorityWitness: async () => "denial-witness",
}));
vi.mock("@erpx/files-server/public/attachment-denied-read-audits", () => ({ attachmentDeniedReadAudits: async () => [] }));
vi.mock("@erpx/files-server/public/private-attachment-reads", () => ({
  attachmentReadSource: async () => h.source,
  attachmentPrivateReadSourceLocks: () => [],
  assertAttachmentReadRoute: () => undefined,
  requirePrivateAttachmentReaderCurrent: async () => undefined,
  bufferedAttachmentRead: (result: unknown) => result,
}));
vi.mock("@erpx/brand-server/adapters/attachment-read-sources", () => ({
  isBrandAttachmentReadTarget: (type: string) => ["holding_brand", "branch_brand", "company_storefront_brand"].includes(type),
  brandAttachmentReadSourceLocks: () => [],
  brandAttachmentReadSourceImage: async () => {
    const value = h.image;
    h.reads += 1;
    if (h.changeImage && h.reads === 1) h.image = "after";
    return value;
  },
}));

const actor = {
  companyId: "company-1", branchId: "branch-1", departmentId: "department-1",
  userId: "user-1", assignmentId: "assignment-1", role: "employee",
  holdingId: "h001", holdingSlug: "acme", iam: undefined, authority: undefined, scope: { kind: "branch", branchSlug: "north" },
} as never;
const root = resolve(__dirname, "../../../");
const read = (path: string) => readFileSync(resolve(root, path), "utf8");

const file = () => {
  const statements = h.statements;
  const db: any = {
    async transaction<T>(action: (unit: typeof db) => Promise<T>) { return action(db); },
    async batch(units: unknown[]) { return { results: units.map(() => ({})), success: true, meta: { changes: 0, duration: 0 } }; },
    prepare(sql: string) {
      const entry = { sql, values: [] as unknown[] };
      statements.push(entry);
      return {
        bind(...values: unknown[]) { entry.values = values; return this; },
        async first<T>() {
          return (/workforce_actor_scope/.test(sql) ? { present: 1 } : null) as T | null;
        },
        async all<T>() { return { results: [] as T[], success: true, meta: { changes: 0, duration: 0 } }; },
      };
    },
  };
  return db;
};
const stored = () => ({
  key: "object-1", size: 1, etag: "etag", httpEtag: null, uploaded: 0,
  httpMetadata: {}, customMetadata: {}, body: new ReadableStream<Uint8Array>(),
  async arrayBuffer() { return new ArrayBuffer(1); },
  async text() { return "x"; },
});

describe("Brand attachment probe / Files consumer verification", () => {
  test("the live read path executes the recorded cross-owner probe with unchanged binds", async () => {
    const { withBrandAttachmentContentRead } = await import("../../../modules/brand/server/src/adapters/attachment-read");
    const { currentAssignmentReadScope } = await import("@erpx/identity-access-server/public/record-source-authority");
    h.statements.length = 0;
    h.image = "before";
    h.reads = 0;
    h.changeImage = false;
    let authorized = false;
    const result = await withBrandAttachmentContentRead(
      file() as never, actor, "link-1", h.source,
      async () => ({ object: stored() as never }),
      async () => { authorized = true; },
    );
    expect((result.object as { key: string }).key).toBe("object-1");
    expect(authorized).toBe(true);
    const scope = currentAssignmentReadScope(actor, "workforce_actor_scope");
    const probe = h.statements.find((entry) => entry.sql.includes("workforce_actor_scope"));
    expect(probe).toBeDefined();
    expect(probe!.sql).toBe(
      `SELECT 1 FROM(VALUES(?::text,?::text,?::text))workforce_actor_scope(company_id,branch_id,department_id) WHERE ${scope.where}`,
    );
    expect(probe!.values).toEqual(["company-1", "branch-1", "department-1", ...scope.args]);
    expect(h.statements.filter((entry) => entry.sql.includes("workforce_actor_scope"))).toHaveLength(1);
  });

  test("a changed Brand source image still fails the read closed", async () => {
    const { withBrandAttachmentContentRead } = await import("../../../modules/brand/server/src/adapters/attachment-read");
    h.statements.length = 0;
    h.image = "before";
    h.reads = 0;
    h.changeImage = true;
    await expect(withBrandAttachmentContentRead(
      file() as never, actor, "link-1", h.source,
      async () => ({ object: stored() as never }),
      async () => undefined,
    )).rejects.toMatchObject({ code: "BRAND_FILE_READ_CHANGED", status: 409 });
    expect(h.reads).toBe(2);
  });

  test("Files and feature-brand consumers stay on native publication paths", () => {
    const service = read("modules/files/server/src/adapters/attachments/service.ts");
    expect(service).toContain("if(selected&&isBrandAttachmentReadTarget(selected.targetType))return withBrandAttachmentContentRead(db,actor,linkId,selected,read,authorize);");
    expect(service).not.toContain("attachmentServiceBrandFacts");
    for (const kind of ["BRAND_PUB_71P", "BRAND_PUB_71I", "BRAND_PUB_72", "BRAND_PUB_73P", "BRAND_PUB_73I", "BRAND_PUB_74"])
      expect(service).toContain(kind);
    const policy = read("modules/files/server/src/adapters/attachments/policy.ts");
    expect(policy).toContain('requireBrandManagement: async (actor, type, id) => await (await import("@erpx/brand-server/public/branding")).requireBrandManagement(db, actor, type, id)');
    const brand = read("modules/brand/server/src/adapters/branding.ts");
    expect(brand).not.toMatch(/\bSELECT\s/);
    expect(brand).toContain("nativeBrandManagementBranch(db,actor,id)");
    expect(brand).toContain("nativeBrandManagementCompanyExists(db,actor,id)");
    for (const path of [
      "modules/brand/ui/src/lib/native-branding.ts",
      "modules/brand/ui/src/adapters/server/native-branding.ts",
      "modules/brand/server/src/adapters/attachment-read-sources.ts",
      "modules/brand/server/src/public/attachment-read.ts",
    ])
      expect(read(path)).not.toMatch(/\bSELECT\s/);
  });

  test("retirement:ts-statements and writer KPI stay at the recorded floor", () => {
    let reads = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".ts")) reads += (readFileSync(path, "utf8").match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(root, "modules/brand/server/src"));
    expect(reads).toBe(0);
    expect(read("modules/brand/server/src/adapters/attachment-read.ts").match(/SELECT /g)).toHaveLength(0);
    expect(read("modules/brand/server/src/adapters/attachment-scope-head.ts").match(/SELECT /g)).toHaveLength(0);
    const writers = read("modules/files/server/src/adapters/attachments/service.ts").match(/attachmentServiceBrandFacts/g) ?? [];
    expect(writers).toHaveLength(0);
  });
});
