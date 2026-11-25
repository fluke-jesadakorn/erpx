import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, test } from "vitest";
import { brandManagementGuardHead } from "../../../packages/server-brand/src/adapters/read-tail-r1";

const root = resolve(__dirname, "../../../");
const srcRoot = resolve(root, "packages/server-brand/src");
const adapters = resolve(srcRoot, "adapters");
const read = (name: string) => readFileSync(resolve(adapters, name), "utf8");
const sha = (s: string) => createHash("sha256").update(s).digest("hex");
const count = (s: string, needle: string) => s.split(needle).length - 1;

// Closed cross-owner projection constant, captured verbatim from the pre-r1
// source. It is passed by the callers exactly as before; r1 adds no import.
const SRC = 'SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes';
const oracle = (columns: string, tail: string) =>
  `SELECT ${columns} FROM (${SRC}) organization_nodes WHERE id=? AND ${tail}`;

type Case = { label: string; columns: string; tail: string; sql: string; bytes: number; sha: string; binds: number };
// Byte oracles recorded from the two retired authority-guard literals
// (BEFORE=3). Only the column list and the predicate tail vary; the head,
// the projection source and every bind stay byte-identical.
const CASES: Case[] = [
  {
    label: "branch management guard",
    columns: "id,display_name",
    tail: "company_id=? AND kind='branch' AND status='active'",
    sql: oracle("id,display_name", "company_id=? AND kind='branch' AND status='active'"),
    bytes: 246,
    sha: "c7bb6a504fd91ecf8a3b555e1d70ccea3a259e9b12cbb0c935e2cc16c9d53b0a",
    binds: 2,
  },
  {
    label: "company existence guard",
    columns: "1",
    tail: "kind='company' AND status='active'",
    sql: oracle("1", "kind='company' AND status='active'"),
    bytes: 216,
    sha: "2ff566d7d22e67c197899ab8c7ebc80a940a2cfdaaadbccae9bb3e50ab4e15c6",
    binds: 1,
  },
];

describe("Brand read-tail r1 dedupe", () => {
  test("r1 renderer reproduces both retired authority-guard literals byte-for-byte", () => {
    expect(CASES).toHaveLength(2);
    for (const c of CASES) {
      const got = brandManagementGuardHead(c.columns, SRC, c.tail);
      expect(got).toBe(c.sql);
      expect(sha(got)).toBe(c.sha);
      expect(Buffer.byteLength(got)).toBe(c.bytes);
      expect((got.match(/\?/g) ?? []).length).toBe(c.binds);
      expect(got.startsWith(`SELECT ${c.columns} FROM (${SRC}) organization_nodes WHERE id=? AND `)).toBe(true);
      expect(got).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE/);
    }
    // Owner-supplied tails keep their own predicates and bind counts verbatim.
    expect(CASES[0].tail).toContain("company_id=?");
    expect(CASES[1].tail).not.toContain("company_id=?");
    expect(brandManagementGuardHead("id,display_name", SRC, CASES[0].tail))
      .not.toBe(brandManagementGuardHead("1", SRC, CASES[1].tail));
  });

  test("both management guards delegate with unchanged tails and bind arrays", () => {
    const src = read("branding.ts");
    expect(src).toContain('import { brandManagementGuardHead } from "./read-tail-r1";');
    expect(src).toContain("brandManagementGuardHead(\"id,display_name\",organizationProjectionSources.organization_nodes,\"company_id=? AND kind='branch' AND status='active'\")");
    expect(src).toContain("brandManagementGuardHead(\"1\",organizationProjectionSources.organization_nodes,\"kind='company' AND status='active'\")");
    expect(src).not.toContain("`SELECT id,display_name FROM (${organizationProjectionSources.organization_nodes})");
    expect(src).not.toContain("`SELECT 1 FROM (${organizationProjectionSources.organization_nodes})");
    expect(count(src, "brandManagementGuardHead(")).toBe(2);

    // Bind arrays and failure behaviour stay at the original call sites.
    expect(src).toContain(".bind(id,actor.companyId).first<{id:string;display_name:string}>()");
    expect(src).toContain(".bind(id).first();}");
    expect(src).toContain('BRAND_MANAGEMENT_FORBIDDEN');

    const helper = read("read-tail-r1.ts");
    expect(count(helper, "export const brandManagementGuardHead =")).toBe(1);
    expect(count(helper, "SELECT ")).toBe(1);
    expect(helper).not.toContain("FOR UPDATE");
    expect(helper).not.toContain("FOR SHARE");
    expect(helper).not.toContain("organizationProjectionSources");
  });

  test("cross-owner probe, locks and native read ports stay untouched", () => {
    // The only excluded residual SELECT is still the inline cross-owner probe.
    const probe = read("attachment-read.ts");
    expect(count(probe, "SELECT ")).toBe(1);
    expect(probe).toContain("`SELECT 1 FROM(VALUES(?::text,?::text,?::text))workforce_actor_scope(company_id,branch_id,department_id) WHERE ${currentScope.where}`");
    expect(probe).toContain(".bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args)");

    // Lock lanes remain unchanged; r1 is a pure text head with no native SQL.
    expect(probe).toContain("lockQueuedCommandAuthority(database)");
    expect(probe).toContain("lockScheduledFeatureSources(database)");
    expect(probe).toContain("brandAttachmentReadSourceLocks(database)");
    expect(read("read-tail-r1.ts")).not.toMatch(/\.rs|nativeBrand/i);
  });

  test("retirement:ts-statements pins 3 -> 2", () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".ts")) total += (readFileSync(path, "utf8").match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r1 folds the two byte-identical management-guard heads into one renderer:
    // -2 inline literals + 1 declaration, 3 - 2 + 1 = 2. The cross-owner probe
    // stays as the single excluded residual.
    expect(total).toBe(2);
    expect(total).toBe(3 - 2 + 1);
    expect(total).toBeLessThan(3);
    expect(count(read("branding.ts"), "SELECT ")).toBe(0);
    expect(count(read("read-tail-r1.ts"), "SELECT ")).toBe(1);
    expect(count(read("attachment-read.ts"), "SELECT ")).toBe(1);
  });
});
