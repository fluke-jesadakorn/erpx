"use client";
import dynamic from "next/dynamic";
export const CountEditor = dynamic(() => import("@erpx/feature-inventory/components/stock-count-controls").then(module => module.CountEditor));
export const TransferEditor = dynamic(() => import("@erpx/feature-inventory/components/stock-transfer-controls").then(module => module.TransferEditor));
export const RequestEditor = dynamic(() => import("@erpx/workplace/components/request-controls").then(module => module.RequestEditor));
export const TaskEditor = dynamic(() => import("@erpx/feature-work_projection/components/staff-work-controls").then(module => module.TaskEditor));
export const LeaveControls = dynamic(() => import("@erpx/feature-people_payroll/components/leave-controls").then(module => module.LeaveControls));
export const ExpenseEditor = dynamic(() => import("@erpx/feature-people_payroll/components/expense-controls").then(module => module.ExpenseEditor));

export const PeopleEditor = dynamic(() => import("@erpx/feature-people_payroll/components/people-editor").then(module => module.PeopleEditor));
export const InvoiceEditor = dynamic(() => import("@erpx/feature-finance/components/invoice-controls").then(module => module.InvoiceEditor));
export const OrderEditor = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.OrderEditor));
export const MasterEditor = dynamic(() => import("@erpx/feature-commerce/components/commerce-controls").then(module => module.MasterEditor));
