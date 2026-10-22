"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const BankMatch = dynamic(() => import("@erpx/feature-finance/components/bank-controls").then(module => module.BankMatch));
