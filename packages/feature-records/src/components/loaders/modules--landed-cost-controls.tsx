"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LandedCostCreateForm = dynamic(() => import("@erpx/feature-commerce/components/landed-cost-controls").then(module => module.LandedCostCreateForm));
export const LandedCostActions = dynamic(() => import("@erpx/feature-commerce/components/landed-cost-controls").then(module => module.LandedCostActions));
