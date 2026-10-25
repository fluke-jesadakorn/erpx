"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FinanceBookControls = dynamic(() => import("@/components/finance/finance-book-controls").then(module => module.FinanceBookControls));
