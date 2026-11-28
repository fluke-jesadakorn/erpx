"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CalculationDecision = dynamic(() => import("@erpx/finance-ui/components/finance-engine-controls").then(module => module.CalculationDecision));
