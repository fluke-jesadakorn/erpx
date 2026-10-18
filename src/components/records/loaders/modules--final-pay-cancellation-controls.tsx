'use client';
import dynamic from 'next/dynamic';
export const FinalPayCancellationControls=dynamic(()=>import('@/components/people_payroll/final-pay-cancellation-controls').then(module=>module.FinalPayCancellationControls));
