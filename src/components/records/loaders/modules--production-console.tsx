"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ProductionConsole = dynamic(() => import("@/components/manufacturing/production-console").then(module => module.ProductionConsole));
