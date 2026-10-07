import type { Asset, AssetState } from "../domain/assets";
import { assetActionSchema } from "../domain/assets";
import type { Row } from "../../commerce/public/commerce";
import type { Clock } from "../../shared/application/ports";

export type AssetCommand = { key: string; operation: string; id: string; input: ReturnType<typeof assetActionSchema.parse> };
export type AssetRepository = {
  replay(command: AssetCommand): Promise<Record<string, unknown> | null>;
  items(): Promise<Asset[]>; people(): Promise<Row[]>; nodes(): Promise<Row[]>; financeCandidates(): Promise<Row[]>;
  asset(id: string): Promise<Asset | null>; finance(id: string): Promise<Row | null>;
  history(id: string): Promise<Row[]>; maintenance(id: string): Promise<Row[]>;
  hasPostedDisposal(financeAssetId: string | null): Promise<boolean>;
  change(command: AssetCommand, source: Asset, state: AssetState, now: number): Promise<Record<string, unknown>>;
};
export function createAssetApplication(ports: {
  actorId: string; authorize(write?: boolean): void; authorizeFinance(): void; authorizeReview(): void;
  canReadFinance(): boolean; canWrite(): boolean; canWriteFinance(): boolean;
  repository(): Promise<AssetRepository>; clock: Clock;
  error(status: number, code: string, message: string): Error;
}) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  async function assetWorkspace() {
    ports.authorize(); const repository = await ports.repository(), items = await repository.items(), people = await repository.people(), nodes = await repository.nodes();
    const financeAssets = ports.canReadFinance() ? await repository.financeCandidates() : [];
    return { items, people, nodes, financeAssets, canLinkFinance: ports.canWrite() && ports.canWriteFinance(), canCreate: ports.canWrite(), actorId: ports.actorId };
  }
  async function assetDetail(id: string) {
    const data = await assetWorkspace(), asset = data.items.find(item => item.id === id);
    if (!asset) return fail(404, "ASSET_NOT_FOUND", "Asset not found in this scope.");
    const repository = await ports.repository(), finance = asset.finance_asset_id && ports.canReadFinance() ? await repository.finance(asset.finance_asset_id) : null;
    return { ...data, asset, finance, history: await repository.history(id), maintenance: await repository.maintenance(id) };
  }
  async function changeAsset(id: string, raw: unknown, key: string) {
    ports.authorize(true); const input = assetActionSchema.parse(raw), repository = await ports.repository(), command = { key, operation: `asset.${input.action}`, id, input }, prior = await repository.replay(command);
    if (prior) return prior;
    const source = await repository.asset(id);
    if (!source) return fail(404, "ASSET_NOT_FOUND", "Asset not found in this scope.");
    const now = ports.clock.now(), state: AssetState = { status: source.lifecycle_status, condition: source.condition, placement: source.placement, custodian: source.custodian_id, retirement: null };
    if (input.action === "link-finance") {
      ports.authorizeFinance();
      if (source.finance_asset_id) return fail(409, "ASSET_FINANCE_LINK_IMMUTABLE", "The recorded Finance asset cannot be replaced.");
    } else if (input.action === "place") {
      if (!["registered", "in_service"].includes(state.status)) return fail(409, "ASSET_PLACEMENT_BLOCKED", "Only available assets can be placed.");
      if (state.condition === "unsafe") return fail(409, "ASSET_UNSAFE", "Resolve the unsafe condition before placing this asset.");
      state.status = "in_service"; state.placement = input.placement; state.custodian = input.custodianId;
    } else if (input.action === "condition") {
      if (state.status === "retired") return fail(409, "ASSET_RETIRED", "A retired asset cannot receive a current condition.");
      state.condition = input.condition;
    } else if (input.action === "retire-request") {
      if (!["registered", "in_service"].includes(state.status)) return fail(409, "ASSET_RETIREMENT_BLOCKED", "Return the asset from maintenance before requesting retirement.");
      state.status = "retirement_pending"; state.retirement = input.reason;
    } else {
      ports.authorizeReview();
      if (state.status !== "retirement_pending") return fail(409, "ASSET_RETIREMENT_NOT_PENDING", "Retirement is not awaiting review.");
      if (Number(source.cost_minor) > 0 && !await repository.hasPostedDisposal(source.finance_asset_id)) return fail(409, "ASSET_FINANCE_DISPOSAL_REQUIRED", "Finance must post the linked asset disposal before physical retirement can be approved.");
      state.status = "retired"; state.retirement = source.retirement_reason; state.placement = null; state.custodian = null;
    }
    return repository.change(command, source, state, now);
  }
  return { assetWorkspace, assetDetail, changeAsset };
}
