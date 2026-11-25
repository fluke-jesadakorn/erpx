import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, test, vi } from "vitest";

/**
 * G4 / retire.brand.attachment-facts — hermetic verification of the one
 * retained Brand cross-owner probe. No statement is cut here: the predicate is
 * owned by Identity Access (`currentAssignmentReadScope`), so moving the
 * envelope without a native owner statement provider would only hide the same
 * cross-owner text in TypeScript. This suite proves the live Files read path
 * still executes the exact recorded probe with unchanged binds, that the
 * image/source gates still fire, and that the Files publication consumers are
 * already native-only (8d71a830a).
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
vi.mock("@erpx/server-commercial/public/purchase-command-boundary", () => ({ companyPurchaseReadLocks: () => [] }));
vi.mock("@erpx/server-commercial/public/package-locks", () => ({ lockPackageSettlementSources: async () => undefined }));
vi.mock("@erpx/server-identity_access/public/queued-command-authority", () => ({ lockQueuedCommandAuthority: async () => undefined }));
vi.mock("@erpx/server-pms/public/scheduled-feature-source-locks", () => ({ lockScheduledFeatureSources: async () => undefined }));
vi.mock("@erpx/server-finance/public/finance-transactions", () => ({ flushFinanceDeferredEffects: async () => undefined }));
vi.mock("@erpx/server-identity_access/public/private-file-denial-authority", () => ({
  privateFileDenialAuthoritySourceLocks: () => [],
  privateFileDenialAuthorityWitness: async () => "denial-witness",
}));
vi.mock("@erpx/server-files/public/attachment-denied-read-audits", () => ({ attachmentDeniedReadAudits: async () => [] }));
vi.mock("@erpx/server-files/public/private-attachment-reads", () => ({
  attachmentReadSource: async () => h.source,
  attachmentPrivateReadSourceLocks: () => [],
  assertAttachmentReadRoute: () => undefined,
  requirePrivateAttachmentReaderCurrent: async () => undefined,
  bufferedAttachmentRead: (result: unknown) => result,
}));
vi.mock("@erpx/server-brand/adapters/attachment-read-sources", () => ({
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
    const { withBrandAttachmentContentRead } = await import("../../../packages/server-brand/src/adapters/attachment-read");
    const { currentAssignmentReadScope } = await import("@erpx/server-identity_access/public/record-source-authority");
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
    const { withBrandAttachmentContentRead } = await import("../../../packages/server-brand/src/adapters/attachment-read");
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
    const service = read("packages/server-files/src/adapters/attachments/service.ts");
    expect(service).toContain("if(selected&&isBrandAttachmentReadTarget(selected.targetType))return withBrandAttachmentContentRead(db,actor,linkId,selected,read,authorize);");
    expect(service).not.toContain("attachmentServiceBrandFacts");
    for (const kind of ["BRAND_PUB_71P", "BRAND_PUB_71I", "BRAND_PUB_72", "BRAND_PUB_73P", "BRAND_PUB_73I", "BRAND_PUB_74"])
      expect(service).toContain(kind);
    const policy = read("packages/server-files/src/adapters/attachments/policy.ts");
    expect(policy).toContain('requireBrandManagement: async (actor, type, id) => await (await import("@erpx/server-brand/public/branding")).requireBrandManagement(db, actor, type, id)');
    const brand = read("packages/server-brand/src/adapters/branding.ts");
    expect(brand).not.toMatch(/\bSELECT\s/);
    expect(brand).toContain("nativeBrandManagementBranch(db,actor,id)");
    expect(brand).toContain("nativeBrandManagementCompanyExists(db,actor,id)");
    for (const path of [
      "packages/feature-brand/src/lib/native-branding.ts",
      "packages/feature-brand/src/adapters/server/native-branding.ts",
      "packages/server-brand/src/adapters/attachment-read-sources.ts",
      "packages/server-brand/src/public/attachment-read.ts",
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
    walk(resolve(root, "packages/server-brand/src"));
    expect(reads).toBe(1);
    expect(read("packages/server-brand/src/adapters/attachment-read.ts").match(/SELECT /g)).toHaveLength(1);
    const writers = read("packages/server-files/src/adapters/attachments/service.ts").match(/attachmentServiceBrandFacts/g) ?? [];
    expect(writers).toHaveLength(0);
  });
});
