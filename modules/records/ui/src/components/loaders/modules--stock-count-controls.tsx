"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CountActions = dynamic(() => import("@erpx/inventory-ui/components/stock-count-controls").then(module => module.CountActions));
export const CountEditor = dynamic(() => import("@erpx/inventory-ui/components/stock-count-controls").then(module => module.CountEditor));
