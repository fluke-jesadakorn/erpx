"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinanceInstrumentControls = dynamic(() => import("@erpx/finance-ui/components/finance-instrument-controls").then(module => module.FinanceInstrumentControls));
