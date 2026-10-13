"use client";
import dynamic from "next/dynamic";
export const CountEditor = dynamic(() => import("@/components/inventory/stock-count-controls").then(module => module.CountEditor));
export const TransferEditor = dynamic(() => import("@/components/inventory/stock-transfer-controls").then(module => module.TransferEditor));
export const RequestEditor = dynamic(() => import("@/components/workplace/request-controls").then(module => module.RequestEditor));
export const TaskEditor = dynamic(() => import("@/components/work_projection/staff-work-controls").then(module => module.TaskEditor));
export const LeaveControls = dynamic(() => import("@/components/people_payroll/leave-controls").then(module => module.LeaveControls));
export const ExpenseEditor = dynamic(() => import("@/components/people_payroll/expense-controls").then(module => module.ExpenseEditor));

export const PeopleEditor = dynamic(() => import("@/components/people_payroll/people-editor").then(module => module.PeopleEditor));
export const InvoiceEditor = dynamic(() => import("@/components/finance/invoice-controls").then(module => module.InvoiceEditor));
export const OrderEditor = dynamic(() => import("@/components/commerce/commerce-controls").then(module => module.OrderEditor));
export const MasterEditor = dynamic(() => import("@/components/commerce/commerce-controls").then(module => module.MasterEditor));
