"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const BookingRefunds = dynamic(() => import("@erpx/pms-ui/components/booking-refunds").then(module => module.BookingRefunds));
