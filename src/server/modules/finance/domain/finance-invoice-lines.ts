import { z } from "zod";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
export const invoiceLineSchema = z.object({
    description: z.string().trim().min(1).max(500),
    quantity: z.string().regex(/^\d+(\.\d{1,3})?$/, "Use a positive quantity with at most three decimals."),
    unitPrice: z.string().regex(/^\d+(\.\d{1,2})?$/, "Use a positive unit price with at most two decimals."),
    taxBps: z.number().int().min(0).max(10000),
    accountId: z.string().min(1).max(128),
    taxAccountId: z.string().min(1).max(128).optional(),
});
export type InvoiceLineInput = z.infer<typeof invoiceLineSchema>;
/** Decimal inputs are multiplied as integers; every line rounds once, half up. */
export function calculateInvoiceLine(input: InvoiceLineInput) {
    const quantityMillis = decimalToMinor(input.quantity, 3);
    const unitPriceMinor = decimalToMinor(input.unitPrice);
    if (quantityMillis <= 0 || unitPriceMinor <= 0)
        throw new Error("Invoice quantities and prices must be positive.");
    const subtotal = (BigInt(quantityMillis) * BigInt(unitPriceMinor) + BigInt(500)) / BigInt(1000);
    const tax = (subtotal * BigInt(input.taxBps) + BigInt(5000)) / BigInt(10000);
    if (subtotal <= BigInt(0) || subtotal + tax > BigInt(Number.MAX_SAFE_INTEGER))
        throw new Error("Invoice line amount exceeds supported precision.");
    return { ...input, quantityMillis, unitPriceMinor, subtotalMinor: Number(subtotal), taxMinor: Number(tax), totalMinor: Number(subtotal + tax) };
}
export function calculateInvoiceLines(inputs: InvoiceLineInput[]) {
    const lines = inputs.map(calculateInvoiceLine);
    const subtotal = lines.reduce((sum, line) => sum + BigInt(line.subtotalMinor), BigInt(0));
    const tax = lines.reduce((sum, line) => sum + BigInt(line.taxMinor), BigInt(0));
    if (subtotal + tax > BigInt(Number.MAX_SAFE_INTEGER))
        throw new Error("Invoice total exceeds supported precision.");
    return { lines, subtotalMinor: Number(subtotal), taxMinor: Number(tax), totalMinor: Number(subtotal + tax), subtotal: minorToDecimal(Number(subtotal)), tax: minorToDecimal(Number(tax)) };
}
