"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const TransitConsole = dynamic(() => import("@erpx/feature-inventory/components/transit-console").then(module => module.TransitConsole));
