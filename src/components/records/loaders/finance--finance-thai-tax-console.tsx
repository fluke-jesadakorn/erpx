"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ThaiVatInvoiceIssue = dynamic(() => import("@/components/finance/finance-thai-tax-console").then(module => module.ThaiVatInvoiceIssue));
export const PaymentWithholdingPanel = dynamic(() => import("@/components/finance/finance-thai-tax-console").then(module => module.PaymentWithholdingPanel));
export const ThaiDocumentControls = dynamic(() => import("@/components/finance/finance-thai-tax-console").then(module => module.ThaiDocumentControls));
