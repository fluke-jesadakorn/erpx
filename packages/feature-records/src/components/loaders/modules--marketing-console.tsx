"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const MarketingConsole = dynamic(() => import("@erpx/feature-marketing/components/marketing-console").then(module => module.MarketingConsole));
