"use client";
import dynamic from "next/dynamic";
export const CountEditor = dynamic(() => import("@erpx/inventory-ui/components/stock-count-controls").then(module => module.CountEditor));
