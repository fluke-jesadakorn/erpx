"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const RequestAi = dynamic(() => import("@/components/workplace/request-ai").then(module => module.RequestAi));
