import { acquisitionCostMinor, assetRegistrationScope, type AssetRegistrationInput } from "../domain/registration";
import type { Clock } from "../../shared/application/ports";

export type AssetRegistrationCommand = { key: string; id: string; operation: "asset.create"; input: AssetRegistrationInput };
export type AssetRegistrationRepository = {
  replay(command: AssetRegistrationCommand): Promise<Record<string, unknown> | null>;
  currencyScale(currency: string): Promise<number>;
  prepareAcquisitionCost(command: AssetRegistrationCommand, costMinor: number): Promise<void>;
  register(command: AssetRegistrationCommand, state: { branch: string; department: string | null; costMinor: number; now: number }): Promise<Record<string, unknown>>;
};
export function createAssetRegistration(ports: {
  authorize(): void; authorizeFinanceLink(): void; parse(raw: unknown): AssetRegistrationInput;
  scope: { branchId: string | null; departmentId: string | null }; clock: Clock;
  decimalUnits(value: string, scale: number): bigint; repository(): Promise<AssetRegistrationRepository>;
  error(status: number, code: string, message: string): Error;
}) {
  return async (raw: unknown, key: string) => {
    ports.authorize(); const input = ports.parse(raw), scope = assetRegistrationScope(ports.scope, input), command: AssetRegistrationCommand = { input, key, id: `asset-${key}`, operation: "asset.create" };
    const repository = await ports.repository(), prior = await repository.replay(command);
    if (prior) return prior;
    let costMinor = input.costMinor;
    if (input.cost !== undefined) {
      const cost = acquisitionCostMinor(ports.decimalUnits(input.cost, await repository.currencyScale(input.currency)));
      if (cost === null) throw ports.error(422, "MONEY_OVERFLOW", "Acquisition cost exceeds supported ledger precision.");
      costMinor = cost;
    }
    await repository.prepareAcquisitionCost(command, costMinor!);
    const now = ports.clock.now();
    if (input.financeAssetId) ports.authorizeFinanceLink();
    return repository.register(command, { ...scope, costMinor: costMinor!, now });
  };
}
