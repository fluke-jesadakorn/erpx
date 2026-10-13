"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupplierBatchConsole = dynamic(() => import("@/components/procurement/supplier-batch-console").then(module => module.SupplierBatchConsole));
