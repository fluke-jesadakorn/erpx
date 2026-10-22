"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ThaiVatInvoiceIssue = dynamic(() => import("@erpx/feature-finance/components/finance-thai-tax-console").then(module => module.ThaiVatInvoiceIssue));
export const PaymentWithholdingPanel = dynamic(() => import("@erpx/feature-finance/components/finance-thai-tax-console").then(module => module.PaymentWithholdingPanel));
export const ThaiDocumentControls = dynamic(() => import("@erpx/feature-finance/components/finance-thai-tax-console").then(module => module.ThaiDocumentControls));
