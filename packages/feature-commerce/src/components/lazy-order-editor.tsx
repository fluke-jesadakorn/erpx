"use client";
import dynamic from "next/dynamic";
export const OrderEditor = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.OrderEditor));
