"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const RefundConsole = dynamic(() => import("@erpx/feature-finance/components/refund-console").then(module => module.RefundConsole));
