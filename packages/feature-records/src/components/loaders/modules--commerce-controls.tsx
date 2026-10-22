"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CustomerDeliveryNotification = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.CustomerDeliveryNotification));
export const StockReservationForm = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.StockReservationForm));
export const FulfillmentForm = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.FulfillmentForm));
export const ServiceCompletionForm = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.ServiceCompletionForm));
export const InvoiceDeliveryForm = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.InvoiceDeliveryForm));
export const OrderActions = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.OrderActions));
export const OrderEditor = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.OrderEditor));
export const ReturnActions = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.ReturnActions));
export const ReturnRequestForm = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.ReturnRequestForm));
export const SettlementForm = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.SettlementForm));
export const MasterEditor = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.MasterEditor));
