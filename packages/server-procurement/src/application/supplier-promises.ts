import { BusinessRuleError, type Clock } from "@erpx/server-shared/index";
import { supplierPromiseSchema, assertPromiseOrder, assertPromiseEvidence, preparePromiseLines, promiseDetailLines, promiseWorkItems, type SupplierPromiseInput, type PromiseRow, type PromiseLine, type PromiseEvidenceSet } from "@erpx/server-procurement/domain/supplier-promises";
export type PromiseMutation<Evidence> = { key: string; operation: string; id: string; promiseId: string; input: SupplierPromiseInput; order: PromiseRow; lines: PromiseLine[]; evidence: Evidence | null; now: number };
export interface SupplierPromisePorts<Evidence> {
  canRead: boolean; canPurchase: boolean; clock: Clock;
  authorizeRead(): Promise<void>; authorizePurchase(): Promise<void>;
  order(id: string): Promise<PromiseRow | null>; sourceLines(id: string): Promise<PromiseRow[]>;
  detail(id: string): Promise<{ lines: PromiseRow[]; history: PromiseRow[]; schedule: PromiseRow[]; open: PromiseRow[] }>;
  evidenceSets(ids: string[]): Promise<PromiseEvidenceSet[]>;
  prepareEvidence(id: string, promiseId: string, input: SupplierPromiseInput, key: string): Promise<Evidence>;
  replay(key: string, operation: string, id: string, input: SupplierPromiseInput): Promise<Record<string, unknown> | null>;
  commit<Result>(mutation: PromiseMutation<Evidence>, result: Result): Promise<Result>;
  workRows(): Promise<PromiseRow[]>;
}
export function createSupplierPromiseApplication<Evidence>(ports: SupplierPromisePorts<Evidence>) {
  const order = async (id: string) => { await ports.authorizeRead(); const value = await ports.order(id); if (!value) throw new BusinessRuleError(404, "PURCHASE_ORDER_NOT_FOUND", "Purchase order not found in this scope."); return value; };
  return {
    order,
    async detail(id: string) { const value = await order(id), data = await ports.detail(id); return { order: value, ...data, lines: promiseDetailLines(data.lines, data.open), canRecord: ports.canPurchase && ["confirmed", "partially_fulfilled"].includes(String(value.state)) }; },
    async record(id: string, raw: unknown, key: string) {
      const input = supplierPromiseSchema.parse(raw); await ports.authorizePurchase(); await ports.authorizeRead();
      const operation = "procurement.promise.record", prior = await ports.replay(key, operation, id, input); if (prior) return prior;
      const value = await order(id); assertPromiseOrder(value, input);
      const source = await ports.sourceLines(id), promiseId = `supplier-promise-${key}`, now = ports.clock.now();
      let evidence: Evidence | null = null;
      if (input.attachmentSetIds.length) { assertPromiseEvidence(input, await ports.evidenceSets(input.attachmentSetIds)); evidence = await ports.prepareEvidence(id, promiseId, input, key); }
      const lines = preparePromiseLines(input, source);
      return ports.commit({ key, operation, id, promiseId, input, order: value, lines, evidence, now }, { id, promiseId, version: input.expectedVersion + 1 });
    },
    async work() { if (!ports.canRead) return []; return promiseWorkItems(await ports.workRows(), ports.canPurchase); },
  };
}
