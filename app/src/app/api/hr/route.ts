import { NextResponse } from 'next/server';
import {
  listEmployees,
  listHRUsers,
  listLeaveRequests,
  listLeaveStats,
  listDeptStats,
} from '@folio-lib/hr/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET() {
  try {
    const [hrUsers, requests, stats, deptStats, employees] = await Promise.all([
      listHRUsers(),
      listLeaveRequests(),
      listLeaveStats(),
      listDeptStats(),
      listEmployees(),
    ]);
    return NextResponse.json({
      success: true,
      hrUsers,
      requests,
      stats,
      deptStats,
      employees,
    });
  } catch (error: unknown) {
    const msg = error instanceof Error ? error.message : String(error);
    console.error('Error fetching HR dashboard data:', msg);
    return NextResponse.json({ success: false, error: msg }, { status: 500 });
  }
}
