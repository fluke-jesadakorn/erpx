"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const OperationsConsole = dynamic(() => import("@/components/operations/operations-console").then(module => module.OperationsConsole));
