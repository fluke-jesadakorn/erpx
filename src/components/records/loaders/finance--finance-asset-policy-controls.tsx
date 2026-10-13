"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinanceAssetPolicyControls = dynamic(() => import("@/components/finance/finance-asset-policy-controls").then(module => module.FinanceAssetPolicyControls));
