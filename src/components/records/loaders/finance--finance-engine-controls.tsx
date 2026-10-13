"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CalculationDecision = dynamic(() => import("@/components/finance/finance-engine-controls").then(module => module.CalculationDecision));
