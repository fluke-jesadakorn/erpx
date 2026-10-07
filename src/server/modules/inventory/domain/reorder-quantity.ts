import {BusinessRuleError} from "../../shared/domain/error";
export function milliQuantity(value: string): string {
  const [whole, fraction = ""] = value.split(".");
  const quantity = BigInt(whole) * BigInt(1000) + BigInt((fraction + "000").slice(0, 3));
  if (quantity <= BigInt(0) || quantity > BigInt("9223372036854775807")) throw new BusinessRuleError(422, "REORDER_POINT_INVALID", "The reorder point must fit a positive inventory quantity.");
  return quantity.toString();
}
