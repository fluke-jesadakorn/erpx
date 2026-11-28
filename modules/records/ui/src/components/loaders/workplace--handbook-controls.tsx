"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const HandbookEditor = dynamic(() => import("@erpx/platform-workplace/components/handbook-controls").then(module => module.HandbookEditor));
