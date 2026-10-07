import { evaluateSalesDiscounts, type SalesPricingInput, type SalesPricingRow } from "../domain/pricing";
import { BusinessRuleError } from "../../shared/domain/error";
export interface SalesPricingPorts {
  list(input: SalesPricingInput): Promise<SalesPricingRow | null>;
  items(priceListId: string | number | null): Promise<SalesPricingRow[]>;
}
export function createSalesPricingApplication(ports: SalesPricingPorts) {
  return async (input: SalesPricingInput) => {
    if (input.kind === "purchase") return { list: null as SalesPricingRow | null, items: new Map<string, SalesPricingRow>(), requiresApproval: false };
    const list = await ports.list(input);
    if (input.priceListId && !list) throw new BusinessRuleError(422, "PRICE_LIST_INVALID", "Choose an active price list valid for this customer and order date.");
    if (!list) return { list: null, items: new Map<string, SalesPricingRow>(), requiresApproval: false };
    const items = new Map((await ports.items(list.id)).map((item) => [String(item.product_id), item]));
    return { list, items, requiresApproval: evaluateSalesDiscounts(list, items, input) };
  };
}
