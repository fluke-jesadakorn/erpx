"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const WorkReturn = dynamic(() => import("@erpx/platform-workplace/components/work-return").then(module => module.WorkReturn));
