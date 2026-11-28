"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const AccrualScheduleConsole = dynamic(() => import("@erpx/finance-ui/components/finance-accrual-controls").then(module => module.AccrualScheduleConsole));
