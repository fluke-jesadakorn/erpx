"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LifecyclePanel = dynamic(() => import("@/components/people/lifecycle-workspace").then(module => module.LifecyclePanel));
