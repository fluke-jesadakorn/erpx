"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupplierPromiseConsole = dynamic(() => import("@erpx/feature-procurement/components/supplier-promise-console").then(module => module.SupplierPromiseConsole));
