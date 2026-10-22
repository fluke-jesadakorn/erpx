"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PeriodControls = dynamic(() => import("@erpx/feature-finance/components/ledger-controls").then(module => module.PeriodControls));
export const JournalAction = dynamic(() => import("@erpx/feature-finance/components/ledger-controls").then(module => module.JournalAction));
export const JournalEditor = dynamic(() => import("@erpx/feature-finance/components/ledger-controls").then(module => module.JournalEditor));
