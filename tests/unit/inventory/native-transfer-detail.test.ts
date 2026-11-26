import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const ROOT = new URL("../../../", import.meta.url).pathname.replace(/\/$/, "");
const read = (path: string) => readFileSync(resolve(ROOT, path), "utf8");
const TS = read("packages/server-inventory/src/adapters/inventory-transfers.ts");
const TRANSPORT = read("packages/server-inventory/src/adapters/native-transfer-detail-read.ts");
const PG = read("packages/server-inventory/src/adapters/native_transfer_detail/postgres.rs");
const HTTP = read("packages/server-inventory/src/adapters/native_transfer_detail/http.rs");
const APP = read("packages/server-inventory/src/adapters/native_transfer_detail/application.rs");
const DOMAIN = read("packages/server-inventory/src/adapters/native_transfer_detail/domain.rs");

describe("native stock-transfer detail payload parity", () => {
  test("the TypeScript SQL authority is deleted and the transport has no SQL", () => {
    expect(TS).toContain("nativeTransferDetail");
    expect(TS).not.toContain("scopedTransfer");
    expect(TS).not.toContain("source_warehouse_name,d.name target_warehouse_name");
    expect(TRANSPORT).toContain("ERPX_NATIVE_INVENTORY_COMMAND_CONFIG");
    expect(TRANSPORT).not.toMatch(/\bSELECT\b|\bFROM\b|\.prepare\(/);
  });

  test("Rust owner SQL retains the complete TS TransferRow projection and scope", () => {
    for (const fragment of [
      "SELECT t.*,s.name source_warehouse_name,d.name target_warehouse_name,chart.base_currency currency",
      "JOIN commerce_warehouses s ON s.id=t.source_warehouse_id",
      "JOIN commerce_warehouses d ON d.id=t.target_warehouse_id",
      "JOIN commerce_settings c ON c.company_id=t.company_id",
      "JOIN finance_charts chart ON chart.id=c.chart_id AND chart.company_id=c.company_id",
      "WHERE t.id=? AND t.company_id=? AND (?::text IS NULL OR t.source_branch_id=? OR t.target_branch_id=?)",
    ]) {
      expect(PG).toContain(fragment);
    }
    expect(DOMAIN).toContain("pub const VALUE_COUNT: usize = 5;");
  });

  test("the full detail row is executed by Rust sqlx, not a plan", () => {
    expect(PG).toContain("SELECT to_jsonb(__transfer_detail)::text");
    expect(PG).toContain("sqlx::query_scalar::<_, String>");
    expect(PG).toContain("fetch_optional(&mut *connection)");
    expect(PG).not.toContain("include_str!");
    expect(HTTP).toContain("read_transfer_detail(reader.as_ref(), &request)");
    expect(APP).toContain("reader.get(request).await?");
  });

  test("missing-row and failure paths stay closed", () => {
    expect(HTTP).toContain("response.row.ok_or_else(not_found)");
    expect(APP).toContain('"TRANSFER_NOT_FOUND"');
    expect(TRANSPORT).toContain("return null;");
  });
});
