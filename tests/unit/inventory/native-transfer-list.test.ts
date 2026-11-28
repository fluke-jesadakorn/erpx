import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const ROOT = new URL("../../../", import.meta.url).pathname.replace(/\/$/, "");
const TS = readFileSync(
  resolve(ROOT, "modules/inventory/server/src/adapters/inventory-transfers.ts"),
  "utf8",
);
const RS = readFileSync(
  resolve(
    ROOT,
    "modules/inventory/server/src/adapters/native_transfer_list/postgres.rs",
  ),
  "utf8",
);

describe("native stock-transfer list payload parity", () => {
  test("Rust owner SQL retains every TS TransferRow field and scope predicate", () => {
    expect(TS).toContain("nativeTransferList");
    expect(TS).not.toContain("t.updated_at");
    for (const fragment of [
      "s.name source_warehouse_name",
      "d.name target_warehouse_name",
      "chart.base_currency currency",
      "t.company_id=? AND (?::text IS NULL OR t.source_branch_id=? OR t.target_branch_id=?)",
      "ORDER BY t.updated_at DESC LIMIT 250",
    ]) {
      expect(RS).toContain(fragment);
    }
  });

  test("the complete row projection is executed by Rust sqlx, not a plan", () => {
    expect(RS).toContain("SELECT to_jsonb(__transfer_list)::text");
    expect(RS).toContain("sqlx::query_scalar::<_, String>");
    expect(RS).toContain("fetch_all(&mut *connection)");
    expect(RS).not.toContain("include_str!");
  });
});
