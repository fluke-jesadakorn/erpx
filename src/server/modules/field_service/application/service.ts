import { BusinessRuleError, type Clock } from "../../shared";
import { agreementActionSchema, agreementTransition, assertOrderAuthority, createOrderSchema, fieldServiceWorkItems, orderActionSchema, orderTransition, type Agreement, type AgreementAction, type BillingTerms, type CreateOrderInput, type FieldRow, type FieldWorkspace, type OrderFacts, type OrderAction, type OrderTransition } from "../domain/service";

type Command = { key: string; operation: string; id: string; now: number };
export type FieldServiceMutation<Consumption, Billing> = Command & (
  | { action: "agreement"; input: AgreementAction; transition: ReturnType<typeof agreementTransition> }
  | { action: "create-order"; input: CreateOrderInput; status: string }
  | { action: "order"; input: OrderAction; order: OrderFacts; transition: OrderTransition; consumption: Consumption | null; billing: Billing | null }
);
export interface FieldServicePorts<Consumption, Billing, Spares> {
  actorId: string; canRead: boolean; canReview: boolean; clock: Clock;
  authorize(write?: boolean): Promise<void>; authorizeReview(): Promise<void>;
  replay(key: string, operation: string, id: string, input: unknown): Promise<Record<string, unknown> | null>;
  workspace(): Promise<FieldWorkspace>; agreement(id: string): Promise<Agreement | null>; order(id: string): Promise<OrderFacts | null>;
  detail(order: FieldWorkspace["orders"][number]): Promise<{ spares: Spares; parts: FieldRow[]; consumptions: FieldRow[]; history: FieldRow[]; billing: FieldRow | null }>;
  reservation(orderId: string, id: string): Promise<FieldRow | null>;
  prepareConsumption(order: OrderFacts, reservation: FieldRow, input: Extract<OrderAction, { action: "issue-part" }>, key: string, now: number): Promise<{ effect: Consumption; id: string; valueMinor: number; journalId: string }>;
  prepareBilling(order: OrderFacts, terms: BillingTerms, key: string, now: number): Promise<Billing>;
  commit<Result>(mutation: FieldServiceMutation<Consumption, Billing>, result: Result): Promise<Result>;
}
export function createFieldServiceApplication<Consumption, Billing, Spares>(ports: FieldServicePorts<Consumption, Billing, Spares>) {
  const workspace = async () => { await ports.authorize(); return ports.workspace(); };
  return {
    workspace,
    async detail(id: string) {
      const data = await workspace(), order = data.orders.find(x => x.id === id);
      if (!order) throw new BusinessRuleError(404, "FIELD_SERVICE_ORDER_NOT_FOUND", "Service order not found in this scope.");
      return { ...data, order, ...await ports.detail(order) };
    },
    async changeAgreement(id: string, raw: unknown, key: string) {
      await ports.authorize(true); const input = agreementActionSchema.parse(raw), operation = `field.agreement.${input.action}`, prior = await ports.replay(key, operation, id, input);
      if (prior) return prior;
      if (!await ports.agreement(id)) throw new BusinessRuleError(404, "FIELD_SERVICE_AGREEMENT_NOT_FOUND", "Agreement not found.");
      await ports.authorizeReview(); const transition = agreementTransition(input), now = ports.clock.now();
      return ports.commit({ action: "agreement", key, operation, id, input, transition, now }, { id, version: input.expectedVersion + 1, status: transition.status });
    },
    async createOrder(raw: unknown, key: string) {
      await ports.authorize(true); const input = createOrderSchema.parse(raw), id = `service-order-${key}`, operation = "field.order.create", prior = await ports.replay(key, operation, id, input);
      if (prior) return prior;
      const now = ports.clock.now(), status = input.scheduledAt ? "scheduled" : "open";
      return ports.commit({ action: "create-order", key, operation, id, input, status, now }, { id, version: 1, status });
    },
    async changeOrder(id: string, raw: unknown, key: string) {
      await ports.authorize(true); const input = orderActionSchema.parse(raw), operation = `field.order.${input.action}`, prior = await ports.replay(key, operation, id, input);
      if (prior) return prior;
      const order = await ports.order(id);
      if (!order) throw new BusinessRuleError(404, "FIELD_SERVICE_ORDER_NOT_FOUND", "Service order not found.");
      const authority = assertOrderAuthority(order, input, ports.actorId, ports.canReview), now = ports.clock.now(), transition = orderTransition(order, input, ports.actorId, authority.manage, now);
      let consumption: Consumption | null = null, billing: Billing | null = null;
      if (input.action === "issue-part") {
        const reservation = await ports.reservation(id, input.reservationId);
        if (!reservation) throw new BusinessRuleError(404, "FIELD_SERVICE_PART_NOT_FOUND", "Select a part reserved for this service order.");
        const prepared = await ports.prepareConsumption(order, reservation, input, key, now); consumption = prepared.effect;
        transition.details = { reservationId: input.reservationId, quantityMilli: input.quantityMilli, consumptionId: prepared.id, costMinor: prepared.valueMinor, journalId: prepared.journalId };
      }
      if (transition.billing) billing = await ports.prepareBilling(order, transition.billing, key, now);
      return ports.commit({ action: "order", key, operation, id, input, order, transition, consumption, billing, now }, { id, version: input.expectedVersion + 1, status: transition.status });
    },
    async work() { if (!ports.canRead) return []; return fieldServiceWorkItems(await workspace(), ports.actorId, ports.canReview); },
  };
}
