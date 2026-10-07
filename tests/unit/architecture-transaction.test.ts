import { describe, expect, test } from "vitest";
import { runAtomicCommand } from "../../src/server/modules/shared/application/command";
import type { CommandIdentity, CommandReceipt, UnitOfWork } from "../../src/server/modules/shared/application/ports";

const identity: CommandIdentity = { key: "command-key", operation: "expense.approve", recordId: "expense-1", input: { expectedVersion: 1 } };
type Result = { version: number };
type Ports = { receipt: CommandReceipt<Result>; expense: () => void; finance: () => void; audit: () => void; outbox: () => void };

function memoryUnitOfWork(failReceipt = false) {
  let committed: string[] = [];
  let receipt: Result | null = null;
  const unitOfWork: UnitOfWork<Ports> = {
    async run(action) {
      const changes: string[] = [];
      let newReceipt = receipt;
      const ports: Ports = {
        expense: () => { changes.push("expense"); },
        finance: () => { changes.push("finance"); },
        audit: () => { changes.push("audit"); },
        outbox: () => { changes.push("outbox"); },
        receipt: {
          async replay() { return receipt ? { result: receipt } : null; },
          async save(_identity, result) { if (failReceipt) throw new Error("receipt unavailable"); newReceipt = result; changes.push("receipt"); },
        },
      };
      const result = await action(ports);
      committed = [...committed, ...changes];
      receipt = newReceipt;
      return result;
    },
  };
  return { unitOfWork, committed: () => committed };
}

describe("transaction-neutral owner ports", () => {
  test("commits a handoff, audit, outbox and receipt through the same ports", async () => {
    const store = memoryUnitOfWork();
    const result = await runAtomicCommand<Result, Ports>(store.unitOfWork, identity, async (ports) => {
      ports.expense(); ports.finance(); ports.audit(); ports.outbox();
      return { version: 2 };
    });
    expect(result).toEqual({ version: 2 });
    expect(store.committed()).toEqual(["expense", "finance", "audit", "outbox", "receipt"]);
    const replay = await runAtomicCommand<Result, Ports>(store.unitOfWork, identity, async () => { throw new Error("replayed action must not execute"); });
    expect(replay).toEqual(result);
    expect(store.committed()).toHaveLength(5);
  });
  test("does not expose a partial cross-owner handoff when action or receipt fails", async () => {
    const store = memoryUnitOfWork();
    await expect(runAtomicCommand<Result, Ports>(store.unitOfWork, identity, async (ports) => { ports.expense(); ports.finance(); throw new Error("finance rejected"); })).rejects.toThrow("finance rejected");
    expect(store.committed()).toEqual([]);
    const receiptFailure = memoryUnitOfWork(true);
    await expect(runAtomicCommand<Result, Ports>(receiptFailure.unitOfWork, identity, async (ports) => { ports.expense(); return { version: 2 }; })).rejects.toThrow("receipt unavailable");
    expect(receiptFailure.committed()).toEqual([]);
  });
});
