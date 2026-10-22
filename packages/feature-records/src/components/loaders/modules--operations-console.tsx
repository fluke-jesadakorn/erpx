"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const OperationsConsole = dynamic(() => import("@erpx/feature-operations/components/operations-console").then(module => module.OperationsConsole));
