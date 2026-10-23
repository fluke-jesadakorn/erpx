"use client";
import dynamic from "next/dynamic";
export const MasterEditor = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.MasterEditor));
