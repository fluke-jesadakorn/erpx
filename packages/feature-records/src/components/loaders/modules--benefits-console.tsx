"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const BenefitsConsole = dynamic(() => import("@erpx/feature-benefits/components/benefits-console").then(module => module.BenefitsConsole));
