"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CounterRefunds = dynamic(() => import("@erpx/pms-ui/components/counter-refunds").then(module => module.CounterRefunds));
