import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const ROOT = new URL("../../../", import.meta.url).pathname.replace(/\/$/, "");
const read = (path: string) => readFileSync(resolve(ROOT, path), "utf8");
const TS = read("modules/inventory/server/src/adapters/inventory-transfers.ts");
const FLOW = read("modules/workflow/server/src/adapters/flow-access.ts");
const TRANSPORT = read("modules/inventory/server/src/adapters/native-transfer-detail-read.ts");
const READ_BARREL = read("modules/inventory/server/src/public/inventory-transfer-read.ts");
const APP_BARREL = read("modules/inventory/server/src/public/inventory-transfers.ts");

describe("native stock-transfer detail caller cutover", () => {
  test("getTransfer no longer accepts or uses a caller SQL unit", () => {
    expect(TS).toContain("export async function getTransfer(actor:WorkContext,id:string)");
    expect(TS).not.toMatch(/function getTransfer\([^)]*SqlDatabase/);
    expect(TS).not.toContain("getTransfer(actor,id,database)");
    expect(TS).not.toContain("getTransfer(actor,id,write)");
  });

  test("Workflow flow access reaches the native port without injecting its unit", () => {
    expect(FLOW).toMatch(/case 'transfer':[^\n]*getTransfer\(actor,id\);break;/);
    expect(FLOW).not.toMatch(/getTransfer\(actor,id,write\)/);
  });

  test("the public getTransfer name is preserved for consumers", () => {
    expect(READ_BARREL).toContain("getTransfer");
    expect(APP_BARREL).toContain("getTransfer");
  });

  test("the sole detail authority is the mounted native transport and it fails closed", () => {
    expect(TS).toContain("nativeTransferDetail(actor,id)");
    expect(TRANSPORT).toContain("native-transfer-detail/read");
    expect(TRANSPORT).toContain("503, 'NATIVE_INVENTORY_TRANSFER_DETAIL_UNAVAILABLE'");
    expect(TRANSPORT).not.toMatch(/\bSELECT\b|\.prepare\(/);
  });
});
