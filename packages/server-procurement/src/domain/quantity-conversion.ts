/** Request and product milli-quantities must agree exactly with the saved factor. */
export function exactConversion(requestMilli: number, productMilli: number, factorMilli: number): boolean {
  return BigInt(requestMilli) * BigInt(1000) === BigInt(productMilli) * BigInt(factorMilli);
}
