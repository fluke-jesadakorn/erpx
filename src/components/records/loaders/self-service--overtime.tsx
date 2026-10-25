"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const OvertimeForm = dynamic(() => import("@/components/self-service/overtime").then(module => module.OvertimeForm));
