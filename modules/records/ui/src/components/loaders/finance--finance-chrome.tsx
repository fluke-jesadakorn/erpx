"use client";
import dynamic from "next/dynamic";

export const FinanceChrome = dynamic(() => import("@erpx/finance-ui/components/finance-chrome").then(module => module.FinanceChrome));
