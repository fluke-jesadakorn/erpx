"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinanceCollectionControls = dynamic(() => import("@erpx/feature-finance/components/finance-collection-controls").then(module => module.FinanceCollectionControls));
