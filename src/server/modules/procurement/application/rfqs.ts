import { BusinessRuleError, type Clock } from "../../shared";
import { rfqCreateSchema, rfqActionSchema, sourceBlocked, currentRfqOffers, independentRfqReviewer, mapRfqDemand, assertRfqSetup, rfqDemand, prepareRfqOffer, assertRfqOrder, rfqWorkItems, type RfqRow, type RfqCreate, type RfqAction, type RfqSource, type RfqDetail, type RfqDemand, type RfqOrderInput } from "../domain/rfqs";

type Command = { key: string; operation: string; id: string; now: number };
type OfferInput = Extract<RfqAction, { action: "offer" }>;
type TransitionInput = Exclude<RfqAction, OfferInput>;
export type RfqMutation<Evidence, Authority> = Command & (
  | { kind: "create"; input: RfqCreate; source: RfqSource | null; warehouse: RfqRow; config: RfqRow; demands: RfqDemand[]; authority: Authority | null }
  | { kind: "offer"; input: OfferInput; rfq: RfqRow; offer: ReturnType<typeof prepareRfqOffer>; evidenceBinding: Evidence | null; status: string; offerId: string | null; evidence: string }
  | { kind: "transition"; input: TransitionInput; rfq: RfqRow; status: string; offerId: string | null; evidence: string }
);
export interface RfqPorts<Evidence, Authority, Stage> {
  clock: Clock; canRead: boolean; canPurchase: boolean; canApprove: boolean; samePerson(id: unknown): boolean;
  authorizeRead(): Promise<void>; authorizePurchase(): Promise<void>; authorizeApprove(): Promise<void>;
  replay(key: string, operation: string, id: string, input: unknown): Promise<Record<string, unknown> | null>;
  rfq(id: string): Promise<RfqRow | null>;
  detail(id: string): Promise<{ lines: RfqRow[]; offers: RfqRow[]; offerLines: RfqRow[]; events: RfqRow[] }>;
  workspace(): Promise<{ rfqs: RfqRow[]; products: RfqRow[]; warehouses: RfqRow[]; suppliers: RfqRow[]; currency: string }>;
  sourceState(id: string): Promise<RfqRow | null>; quotationSource(id: string): Promise<RfqSource>; currentQuotationSource(id: string): Promise<RfqSource | null>;
  configuration(): Promise<RfqRow | null>; warehouse(id: string): Promise<RfqRow | null>; product(id: string): Promise<RfqRow | null>; supplier(id: string, currency: string): Promise<RfqRow | null>;
  prepareSource(source: RfqSource, key: string): Promise<Authority>;
  prepareEvidence(id: string, offerId: string, input: OfferInput, key: string): Promise<Evidence>;
  commit<Result>(mutation: RfqMutation<Evidence, Authority>, result: Result): Promise<Result>;
  linkedRfq(orderId: string): Promise<RfqRow | null>;
  stageHandoff(input: { detail: RfqDetail; selected: RfqRow; order: RfqOrderInput; orderId: string; key: string; editing: boolean; now: number }): Promise<Stage[]>;
  workRows(): Promise<RfqRow[]>;
}
export function createRfqApplication<Evidence, Authority, Stage>(ports: RfqPorts<Evidence, Authority, Stage>) {
  const today = () => new Date(ports.clock.now()).toISOString().slice(0, 10);
  const get = async (id: string) => { await ports.authorizeRead(); const rfq = await ports.rfq(id); if (!rfq) throw new BusinessRuleError(404, "RFQ_NOT_FOUND", "Quotation request not found in this scope."); return rfq; };
  const detail = async (id: string): Promise<RfqDetail> => {
    const rfq = await get(id), data = await ports.detail(id), current = currentRfqOffers(data.offers), state = rfq.request_id ? await ports.sourceState(String(rfq.request_id)) : null;
    const blocked = sourceBlocked(rfq, state), independent = independentRfqReviewer(rfq, data.offers, ports.samePerson);
    let currentRequestVersion: number | null = null;
    if (rfq.request_id && rfq.status === "approved") { const source = await ports.currentQuotationSource(String(rfq.request_id)); if (source && source.request.evidence_version === Number(rfq.request_evidence_version) && source.request.items_json === rfq.request_items_json) currentRequestVersion = source.request.version; }
    return { rfq, currentRequestVersion, sourceBlocked: blocked, ...data, current, canPrepare: ports.canPurchase, canApprove: ports.canPurchase && ports.canApprove && independent && !blocked };
  };
  return {
    get, detail,
    async workspace() { await ports.authorizeRead(); return { ...await ports.workspace(), canPrepare: ports.canPurchase }; },
    async create(raw: unknown, key: string) {
      const input = rfqCreateSchema.parse(raw); await ports.authorizePurchase(); const id = `rfq-${key}`, operation = "procurement.rfq.create", prior = await ports.replay(key, operation, id, input); if (prior) return prior;
      const source = input.requestId ? await ports.quotationSource(input.requestId) : null;
      if (source && (source.request.version !== input.requestVersion || source.request.currency && source.request.currency !== String((await ports.configuration())?.currency))) throw new BusinessRuleError(409, "RFQ_REQUEST_CHANGED", "Refresh the approved requisition and use its currency.");
      const mapped = mapRfqDemand(input, source), warehouse = await ports.warehouse(input.warehouseId), config = await ports.configuration(), setup = assertRfqSetup(input, source, warehouse, config), now = ports.clock.now();
      const authority = source ? await ports.prepareSource(source, key) : null, demands: RfqDemand[] = [];
      for (const [ordinal, line] of input.lines.entries()) demands.push(rfqDemand(line, await ports.product(line.productId), ordinal, mapped[ordinal] ?? null));
      return ports.commit({ kind: "create", key, operation, id, input, source, warehouse: setup.warehouse, config: setup.config, demands, authority, now }, { id, version: 1 });
    },
    async act(id: string, raw: unknown, key: string) {
      const input = rfqActionSchema.parse(raw); await ports.authorizePurchase(); const operation = `procurement.rfq.${input.action}`, prior = await ports.replay(key, operation, id, input); if (prior) return prior;
      const data = await detail(id), rfq = data.rfq;
      if (data.sourceBlocked && input.action !== "cancel") throw new BusinessRuleError(409, "RFQ_REQUEST_CHANGED", "The requisition is no longer approved and ready with the same demand. Resolve the source or cancel this RFQ.");
      let status = String(rfq.status), offerId = rfq.selected_offer_id ? String(rfq.selected_offer_id) : null;
      if (input.action === "offer") {
        if (status !== "collecting") throw new BusinessRuleError(409, "RFQ_LOCKED", "Return the award to collection before recording another offer.");
        const offer = prepareRfqOffer(input, data, await ports.supplier(input.supplierId, String(rfq.currency)), today()), createdOffer = `rfq-offer-${key}`;
        const evidenceBinding = input.attachmentSetIds.length ? await ports.prepareEvidence(id, createdOffer, input, key) : null;
        return ports.commit({ kind: "offer", key, operation, id, input, rfq, offer, evidenceBinding, status, offerId, evidence: input.evidence, now: ports.clock.now() }, { id, version: input.expectedVersion + 1, status });
      }
      if (input.action === "submit") {
        const chosen = data.current.find(offer => offer.id === input.offerId);
        if (status !== "collecting" || !chosen || String(chosen.valid_until) < today()) throw new BusinessRuleError(409, "RFQ_CURRENT_OFFER_REQUIRED", "Select a current unexpired supplier offer while collecting.");
        offerId = String(chosen.id); status = "review";
      } else if (input.action === "approve") {
        await ports.authorizeApprove();
        if (status !== "review" || !data.canApprove) throw new BusinessRuleError(403, "RFQ_INDEPENDENT_APPROVER_REQUIRED", "A different purchasing manager must review the creator, proposer and selected offer recorder.");
        const selected = data.current.find(offer => offer.id === offerId);
        if (!selected || String(selected.valid_until) < today()) throw new BusinessRuleError(409, "RFQ_QUOTE_EXPIRED", "The selected quotation expired. Return it for a replacement."); status = "approved";
      } else if (input.action === "return") {
        await ports.authorizeApprove(); if (!["review", "approved"].includes(status)) throw new BusinessRuleError(409, "RFQ_RETURN_INVALID", "Only an unconsumed review or award can return to collection."); status = "collecting"; offerId = null;
      } else {
        if (!["collecting", "review", "approved"].includes(status)) throw new BusinessRuleError(409, "RFQ_CANCEL_INVALID", "The purchase order owns cancellation after handoff.");
        if (status === "approved") { await ports.authorizeApprove(); if (ports.samePerson(rfq.created_by)) throw new BusinessRuleError(403, "RFQ_INDEPENDENT_APPROVER_REQUIRED", "A different manager must cancel an approved award."); } status = "cancelled";
      }
      return ports.commit({ kind: "transition", key, operation, id, input, rfq, status, offerId, evidence: input.reason, now: ports.clock.now() }, { id, version: input.expectedVersion + 1, status });
    },
    async handoff(input: RfqOrderInput, orderId: string, key: string, editing: boolean) {
      const linked = editing ? await ports.linkedRfq(orderId) : null, id = linked?.id ?? input.rfqId; if (!id) return [];
      const data = await detail(String(id)), selected = assertRfqOrder(data, input, orderId, editing, today());
      if ((await ports.configuration())?.currency !== data.rfq.currency) throw new BusinessRuleError(409, "RFQ_CURRENCY_CHANGED", "Review the award after commerce currency changes.");
      return ports.stageHandoff({ detail: data, selected, order: input, orderId, key, editing, now: ports.clock.now() });
    },
    async work() { if (!ports.canRead) return []; return rfqWorkItems(await ports.workRows(), ports.canPurchase, ports.canApprove, ports.samePerson); },
  };
}
