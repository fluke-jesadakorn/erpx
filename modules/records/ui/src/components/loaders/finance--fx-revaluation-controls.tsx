"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FxRevaluationControls = dynamic(() => import("@erpx/finance-ui/components/fx-revaluation-controls").then(module => module.FxRevaluationControls));
