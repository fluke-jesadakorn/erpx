import type { CommandIdentity, CommandReceipt, UnitOfWork } from "./ports";

/** Receipt checks and changes share the adapter's transaction and serialization. */
export async function runAtomicCommand<Result, Ports extends { receipt: CommandReceipt<Result> }>(
  unitOfWork: UnitOfWork<Ports>,
  identity: CommandIdentity,
  action: (ports: Ports) => Promise<Result>,
): Promise<Result> {
  return unitOfWork.run(async (ports) => {
    const prior = await ports.receipt.replay(identity);
    if (prior) return prior.result;
    const result = await action(ports);
    await ports.receipt.save(identity, result);
    return result;
  });
}
