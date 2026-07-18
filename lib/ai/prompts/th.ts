import type { SqlSummaryTemplates } from './en';

export const th: SqlSummaryTemplates = {
  empty: 'ไม่พบข้อมูลที่ตรงกับคำถาม',
  count: ({ noun, formatted }) =>
    `พบ${noun ? ' ' + noun : ''}ทั้งหมด ${formatted} รายการ`,
  average: ({ formatted }) =>
    `ค่าเฉลี่ยอยู่ที่ ${formatted}`,
  sum: ({ formatted }) =>
    `ยอดรวม ${formatted} บาท`,
  result: ({ bullets }) => `ผลลัพธ์: ${bullets}`,
  fallback: ({ rows }) => `พบข้อมูล ${rows} รายการ`,
};