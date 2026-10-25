"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinancePlanningControls = dynamic(() => import("@/components/finance/finance-planning-controls").then(module => module.FinancePlanningControls));
