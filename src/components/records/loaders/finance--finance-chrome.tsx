"use client";
import dynamic from "next/dynamic";

export const FinanceChrome = dynamic(() => import("@/components/finance/finance-chrome").then(module => module.FinanceChrome));
