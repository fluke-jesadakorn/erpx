"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const RfqConsole = dynamic(() => import("@/components/procurement/procurement-rfq-console").then(module => module.RfqConsole));
