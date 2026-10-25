"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const BenefitsConsole = dynamic(() => import("@/components/modules/benefits-console").then(module => module.BenefitsConsole));
