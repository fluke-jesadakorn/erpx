import "server-only";
import { scopedReadHead } from "./commercial-read-tail-r14-sources";
/** Parameterized id-scoped receipt/existence probe family shared by the
 * Commercial grant store (G4 read-tail r5). The single scoped head owns the SQL
 * literal; every consumer still prepares and binds its own independent statement
 * and keeps its exports. Arguments are code-owned table/column fragments only,
 * never request input. The r1 read now delegates here with one unchanged bind.
 * Pure read: no writes, no table locks and no advisory locks. */
export function receiptProbe(table: string, columns: string) {
  return scopedReadHead(columns, table, 'id');
}
