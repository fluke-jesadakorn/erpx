"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinanceBusinessContractControls = dynamic(() => import("@/components/finance/finance-business-contract-controls").then(module => module.FinanceBusinessContractControls));
