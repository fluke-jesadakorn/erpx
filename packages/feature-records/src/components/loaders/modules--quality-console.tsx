"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const QualityConsole = dynamic(() => import("@erpx/feature-quality/components/quality-console").then(module => module.QualityConsole));
