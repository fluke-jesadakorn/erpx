"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const BookingRefunds = dynamic(() => import("@/components/pms/booking-refunds").then(module => module.BookingRefunds));
