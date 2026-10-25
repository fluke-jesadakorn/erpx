"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinanceGroupEditionControls = dynamic(() => import("@/components/finance/finance-group-edition-controls").then(module => module.FinanceGroupEditionControls));
