"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinanceBankAutomationControls = dynamic(() => import("@erpx/feature-finance/components/finance-bank-automation-controls").then(module => module.FinanceBankAutomationControls));
