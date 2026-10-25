"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SalesPricingConsole = dynamic(() => import("@/components/modules/sales-pricing-console").then(module => module.SalesPricingConsole));
