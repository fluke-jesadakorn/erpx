"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinanceAssetConsole = dynamic(() => import("@/components/modules/finance-asset-console").then(module => module.FinanceAssetConsole));
