"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupplierBillCapture = dynamic(() => import("@/components/modules/supplier-bill-console").then(module => module.SupplierBillCapture));
export const SupplierBillConsole = dynamic(() => import("@/components/modules/supplier-bill-console").then(module => module.SupplierBillConsole));
