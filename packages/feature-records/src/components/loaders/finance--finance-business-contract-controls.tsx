"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinanceBusinessContractControls = dynamic(() => import("@erpx/feature-finance/components/finance-business-contract-controls").then(module => module.FinanceBusinessContractControls));
