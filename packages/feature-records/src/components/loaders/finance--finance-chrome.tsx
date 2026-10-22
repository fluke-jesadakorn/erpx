"use client";
import dynamic from "next/dynamic";

export const FinanceChrome = dynamic(() => import("@erpx/feature-finance/components/finance-chrome").then(module => module.FinanceChrome));
