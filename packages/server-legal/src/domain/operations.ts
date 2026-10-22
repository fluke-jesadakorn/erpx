import {z} from "zod";
import {parseItem,type ItemInput} from "@erpx/server-legal/domain/model";
export function previewLegalImport(raw: unknown,hash:(text:string)=>string) {
  const rows = z.array(z.unknown()).min(1).max(100).parse(raw),
    valid: ItemInput[] = [],
    errors: { row: number; message: string }[] = [];
  rows.forEach((r, i) => {
    try {
      valid.push(parseItem(r));
    } catch (e) {
      errors.push({
        row: i + 1,
        message: e instanceof Error ? e.message : "Invalid record",
      });
    }
  });
  return {
    rows: valid,
    errors,
    digest: hash(JSON.stringify(valid)),
    count: rows.length,
  };
}
