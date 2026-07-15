import type { MessageDict } from './types';

const dict: MessageDict = {
  en: {

    'matrix.title': 'Stage Chain',
    'matrix.grantsN': '{n} grants',
    'matrix.subtitle': 'Toggle a stage permission per persona. Saving replaces the role’s allow-grant set and writes an audit row per change.',
    'matrix.chainOrder': 'chain order →',
    'matrix.editGranted': 'rbac:matrix:edit granted',
    'matrix.viewOnly': 'rbac:matrix:view only',
    'matrix.colPersona': 'Persona',
    'matrix.colUsers': 'Users',
    'matrix.colActions': 'Actions',
    'matrix.reset': 'Reset',
    'matrix.save': 'Save',
    'matrix.saving': 'Saving…',
    'matrix.howItWorks': 'How this works',
    'matrix.howItWorksBody': 'Each row is a perm.roles row with kind = ‘persona’. Each column is one of the eight stage:*:act:all permissions. Saving replaces the role’s allow-set and writes an audit row per change. Receipt-chain order is fixed in lib/perm/stages.ts::STAGE_ORDER.',

  },
  th: {

    'matrix.title': 'ลำดับขั้นตอน',
    'matrix.grantsN': '{n} สิทธิ์',
    'matrix.subtitle': 'สลับสิทธิ์ขั้นตอนต่อบุคคล การบันทึกจะแทนที่ชุดสิทธิ์ allow ของบทบาทและเขียนแถวบันทึกการตรวจสอบต่อการเปลี่ยนแปลง',
    'matrix.chainOrder': 'ลำดับขั้นตอน →',
    'matrix.editGranted': 'rbac:matrix:edit ได้รับสิทธิ์',
    'matrix.viewOnly': 'rbac:matrix:view เท่านั้น',
    'matrix.colPersona': 'บุคคล',
    'matrix.colUsers': 'ผู้ใช้',
    'matrix.colActions': 'การดำเนินการ',
    'matrix.reset': 'รีเซ็ต',
    'matrix.save': 'บันทึก',
    'matrix.saving': 'กำลังบันทึก…',
    'matrix.howItWorks': 'วิธีการทำงาน',
    'matrix.howItWorksBody': 'แต่ละแถวคือแถวใน perm.roles ที่มี kind = ‘persona’ แต่ละคอลัมน์คือหนึ่งในแปดสิทธิ์ stage:*:act:all การบันทึกจะแทนที่ allow-set และเขียนแถวบันทึกการตรวจสอบต่อการเปลี่ยนแปลง ลำดับสายใบเสร็จถูกกำหนดใน lib/perm/stages.ts::STAGE_ORDER',

  },
  de: {

    'matrix.title': 'Stufenkette',
    'matrix.grantsN': '{n} Berechtigungen',
    'matrix.subtitle': 'Stufenberechtigung pro Persona umschalten. Speichern ersetzt den Allow-Grant-Satz der Rolle und schreibt eine Audit-Zeile pro Änderung.',
    'matrix.chainOrder': 'Kettenreihenfolge →',
    'matrix.editGranted': 'rbac:matrix:edit gewährt',
    'matrix.viewOnly': 'nur rbac:matrix:view',
    'matrix.colPersona': 'Persona',
    'matrix.colUsers': 'Benutzer',
    'matrix.colActions': 'Aktionen',
    'matrix.reset': 'Zurücksetzen',
    'matrix.save': 'Speichern',
    'matrix.saving': 'Speichern…',
    'matrix.howItWorks': 'So funktioniert es',
    'matrix.howItWorksBody': 'Jede Zeile ist eine perm.roles-Zeile mit kind = ‘persona’. Jede Spalte ist eine der acht stage:*:act:all-Berechtigungen. Speichern ersetzt den Allow-Satz und schreibt eine Audit-Zeile pro Änderung. Die Belegketten-Reihenfolge ist in lib/perm/stages.ts::STAGE_ORDER festgelegt.',

    'permissions.page.tiles.sign_in':              'Zum Anzeigen dieser Seite anmelden.',
    'permissions.page.tiles.view_required':        'rbac:matrix:view erforderlich.',
  },
};

export default dict;
