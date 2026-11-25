import { describe, it, expect } from "vitest";
import { readFile, readdir } from "node:fs/promises";
import { resolve } from "node:path";

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const srcRoot = resolve("packages/server-marketing/src");

// Byte oracles captured from the two original inline statements before dedupe.
const exists =
  "SELECT 1 FROM assignments WHERE id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text))";
const existsSelected =
  "SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array('marketing'::text))";
const allowed =
  "SELECT (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?";

async function selectTokens(): Promise<number> {
  let total = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith(".ts")) total += ((await readFile(path, "utf8")).match(/SELECT /g) ?? []).length;
    }
  };
  await walk(srcRoot);
  return total;
}

describe("marketing module-membership probe dedupe (r37)", () => {
  it("emits byte-identical EXISTS and ALLOWED probes from one renderer", async () => {
    const { marketingModuleMemberSql } = await import(
      "../../../packages/server-marketing/src/adapters/module-membership.ts"
    );
    expect(marketingModuleMemberSql()).toBe(exists);
    expect(marketingModuleMemberSql("selected")).toBe(existsSelected);
    expect(marketingModuleMemberSql("", "allowed")).toBe(allowed);
  });

  it("keeps one statement token and one closed modules_json predicate in the helper", async () => {
    const helper = await readFile(adapter("module-membership.ts"), "utf8");
    expect(helper.match(/SELECT /g)).toBeNull();
    expect(helper).toContain("marketingSelectHeadSql(head, tail)");
    const head = await readFile(adapter("select-head-sql.ts"), "utf8");
    expect(head.match(/SELECT /g) ?? []).toHaveLength(1);
    expect(helper.match(/jsonb_build_array\('marketing'::text\)/g)).toHaveLength(1);
  });

  it("removes the inline ALLOWED literal from purchases.ts and keeps its bind order", async () => {
    const purchases = await readFile(adapter("purchases.ts"), "utf8");
    expect(purchases).not.toContain(allowed);
    expect(purchases).toContain('marketingModuleMemberSql("", "allowed")');
    expect(purchases).toContain('.bind(actor.assignmentId).first<{allowed:boolean}>()');
  });

  it("leaves the four EXISTS consumers and their aliases untouched", async () => {
    for (const name of [
      "analytics-sources.ts",
      "attachment-purchases.ts",
      "metric-purchases.ts",
      "record-purchases.ts",
    ]) {
      const source = await readFile(adapter(name), "utf8");
      expect(source).toContain("marketingModuleMemberSql(");
      expect(source).not.toContain(allowed);
    }
  });

  it("retirement:ts-statements KPI stays strictly below BEFORE=65", async () => {
    // r38 lowers the owner tail further (62); this round only pins the baseline drop.
    expect(await selectTokens()).toBeLessThan(65);
  });
});
