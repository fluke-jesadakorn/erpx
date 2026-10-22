"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const RfqConsole = dynamic(() => import("@erpx/feature-procurement/components/procurement-rfq-console").then(module => module.RfqConsole));
