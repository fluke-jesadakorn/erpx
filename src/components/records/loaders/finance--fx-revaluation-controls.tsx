"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FxRevaluationControls = dynamic(() => import("@/components/finance/fx-revaluation-controls").then(module => module.FxRevaluationControls));
