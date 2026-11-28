"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinanceRecordWorkspace = dynamic(() => import("@erpx/finance-ui/components/finance-record-workspace").then(module => module.FinanceRecordWorkspace));
export const FinanceLegacyHeading = dynamic(() => import("@erpx/finance-ui/components/finance-legacy-heading").then(module => module.FinanceLegacyHeading));
