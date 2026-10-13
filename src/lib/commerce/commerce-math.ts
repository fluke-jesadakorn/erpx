import { decimalToMinor } from "@/lib/finance/finance-money";
export function quantityMilli(value:string) { const n=decimalToMinor(value,3);if(n<=0)throw new Error("Quantity must be positive.");return n; }
export function scaledAmount(amount:number, numerator:number, denominator:number) {
  const n=(BigInt(amount)*BigInt(numerator)+BigInt(Math.floor(denominator/2)))/BigInt(denominator);
  if(n>BigInt(Number.MAX_SAFE_INTEGER))throw new Error("Amount is too large."); return Number(n);
}
export function lineAmounts(quantity:number,price:number,taxBps:number) { const subtotal=scaledAmount(price,quantity,1000);return {subtotal,tax:scaledAmount(subtotal,taxBps,10000)}; }
