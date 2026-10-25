"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const OperationsConsole = dynamic(() => import("@/components/modules/operations-console").then(module => module.OperationsConsole));
