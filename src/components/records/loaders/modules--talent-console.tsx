"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const TalentConsole = dynamic(() => import("@/components/modules/talent-console").then(module => module.TalentConsole));
