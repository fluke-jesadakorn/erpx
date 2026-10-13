import { decimalToMinor } from "@/lib/finance/finance-money";
import { BusinessRuleError } from "../../shared/domain/error";
export type SalesPricingRow = Record<string, string | number | null>;
export type SalesPricingInput = { kind: "quote" | "sales" | "purchase"; partyId: string; orderDate: string; priceListId?: string; discountReason?: string; lines: Array<{ productId: string; price: string; taxBps: number }> };
export function evaluateSalesDiscounts(list: SalesPricingRow, items: Map<string, SalesPricingRow>, input: SalesPricingInput): boolean {
  const discounts = input.lines.map((line) => {
    const item = items.get(line.productId);
    if (!item) throw new BusinessRuleError(422, "PRICE_LIST_ITEM_REQUIRED", "Every order product must exist on the selected price list.");
    if (Number(item.tax_bps) !== line.taxBps) throw new BusinessRuleError(422, "PRICE_LIST_TAX_REQUIRED", "Use the approved tax treatment captured by the price list.");
    const price = decimalToMinor(line.price), base = Number(item.price_minor);
    return base > 0 && price < base ? Math.floor((base - price) * 10000 / base) : 0;
  });
  const requiresApproval = discounts.some((value) => value > Number(list.max_auto_discount_bps));
  if (requiresApproval && (!input.discountReason || input.discountReason.trim().length < 5)) throw new BusinessRuleError(422, "DISCOUNT_REASON_REQUIRED", "Explain why this order exceeds the automatic discount threshold.");
  return requiresApproval;
}
