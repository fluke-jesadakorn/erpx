import type { Asset } from "@erpx/server-assets/domain/assets";
import type {Row} from '@erpx/server-commerce/public/commerce';

export type AssetRepository = {
  items(): Promise<Asset[]>; people(): Promise<Row[]>; nodes(): Promise<Row[]>; financeCandidates(): Promise<Row[]>;
  finance(id: string): Promise<Row | null>;
  history(id: string): Promise<Row[]>; maintenance(id: string): Promise<Row[]>;
};
export function createAssetApplication(ports: {
  actorId: string; authorize(write?: boolean): void;
  canReadFinance(): boolean; canWrite(): boolean; canWriteFinance(): boolean;
  repository(): Promise<AssetRepository>;
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
  return { assetWorkspace, assetDetail };
}
