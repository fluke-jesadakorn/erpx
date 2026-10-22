"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ConsolidationRunConsole = dynamic(() => import("@erpx/feature-finance/components/consolidation-run-console").then(module => module.ConsolidationRunConsole));
