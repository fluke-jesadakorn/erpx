import { query } from '@/db';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface LeaveExportRow {
  'รหัสพนักงาน': string;
  'ชื่อ-สกุล': string;
  'แผนก': string;
  'ตำแหน่ง': string;
  'ประเภทการลา': string;
  'วันเริ่ม': string;
  'วันสิ้นสุด': string;
  'จำนวนวัน': string | number;
  'เหตุผล': string;
  'สถานะ': string;
  'อนุมัติโดย': string | null;
  'วันที่ส่งคำขอ': string;
}

export async function GET(req: Request) {
  try {
    const { searchParams } = new URL(req.url);
    const month = searchParams.get('month');

    let sql = `
      SELECT
        e.employee_code      AS "รหัสพนักงาน",
        e.name               AS "ชื่อ-สกุล",
        e.department         AS "แผนก",
        e.position           AS "ตำแหน่ง",
        lr.leave_type        AS "ประเภทการลา",
        lr.start_date::text  AS "วันเริ่ม",
        lr.end_date::text    AS "วันสิ้นสุด",
        lr.days::float       AS "จำนวนวัน",
        lr.reason            AS "เหตุผล",
        lr.status            AS "สถานะ",
        appr.name            AS "อนุมัติโดย",
        lr.created_at        AS "วันที่ส่งคำขอ"
      FROM hr.leave_requests lr
      JOIN hr.employees e ON lr.employee_id = e.id
      LEFT JOIN hr.employees appr ON lr.approved_by = appr.id
    `;

    const params: string[] = [];
    if (month) {
      sql += ` WHERE TO_CHAR(lr.created_at, 'YYYY-MM') = $1`;
      params.push(month);
    }
    sql += ` ORDER BY lr.created_at DESC`;

    const result = await query<LeaveExportRow>(sql, params);
    const rows = result.rows;

    if (rows.length === 0) {
      const period = month || 'all';
      return new Response('ไม่พบข้อมูลในช่วงเวลาที่เลือก', {
        headers: {
          'Content-Type': 'text/plain; charset=utf-8',
          'Content-Disposition': `attachment; filename="leave-report-${period}.csv"`,
        },
      });
    }

    const leaveTypeThai: Record<string, string> = {
      sick: 'ลาป่วย',
      annual: 'ลาพักร้อน',
      personal: 'ลากิจ',
    };
    const statusThai: Record<string, string> = {
      pending: 'รออนุมัติ',
      approved: 'อนุมัติแล้ว',
      rejected: 'ปฏิเสธแล้ว',
    };

    const columns = [
      'รหัสพนักงาน',
      'ชื่อ-สกุล',
      'แผนก',
      'ตำแหน่ง',
      'ประเภทการลา',
      'วันเริ่ม',
      'วันสิ้นสุด',
      'จำนวนวัน',
      'เหตุผล',
      'สถานะ',
      'อนุมัติโดย',
      'วันที่ส่งคำขอ',
    ];

    const escapeCell = (val: unknown): string => {
      if (val === null || val === undefined) return '';
      const str = String(val);
      if (str.includes(',') || str.includes('"') || str.includes('\n')) {
        return `"${str.replace(/"/g, '""')}"`;
      }
      return str;
    };

    const csvLines: string[] = [columns.join(',')];

    for (const row of rows) {
      const leaveType = leaveTypeThai[row['ประเภทการลา']] || row['ประเภทการลา'];
      const status = statusThai[row['สถานะ']] || row['สถานะ'];
      const createdAt = row['วันที่ส่งคำขอ']
        ? new Date(row['วันที่ส่งคำขอ']).toISOString().split('T')[0]
        : '';

      const cells = [
        escapeCell(row['รหัสพนักงาน']),
        escapeCell(row['ชื่อ-สกุล']),
        escapeCell(row['แผนก']),
        escapeCell(row['ตำแหน่ง']),
        escapeCell(leaveType),
        escapeCell(row['วันเริ่ม']),
        escapeCell(row['วันสิ้นสุด']),
        escapeCell(row['จำนวนวัน']),
        escapeCell(row['เหตุผล']),
        escapeCell(status),
        escapeCell(row['อนุมัติโดย']),
        escapeCell(createdAt),
      ];
      csvLines.push(cells.join(','));
    }

    const csvContent = '\uFEFF' + csvLines.join('\r\n');
    const period = month || 'all';

    return new Response(csvContent, {
      headers: {
        'Content-Type': 'text/csv; charset=utf-8',
        'Content-Disposition': `attachment; filename="leave-report-${period}.csv"`,
      },
    });
  } catch (error: unknown) {
    const msg = error instanceof Error ? error.message : String(error);
    console.error('Error generating CSV export:', msg);
    return new Response(JSON.stringify({ success: false, error: msg }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}
