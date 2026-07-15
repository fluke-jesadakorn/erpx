-- db/2026-07-15-de-locale-seed.sql
-- Backfill perm.roles.display_name_de with proper German translations.
-- Idempotent: WHERE clauses make re-runs safe.
BEGIN;

UPDATE perm.roles SET display_name_de = 'Geschäftsführung' WHERE id = 'dept-executive';
UPDATE perm.roles SET display_name_de = 'Finanzen & Buchhaltung' WHERE id = 'dept-finance-2';
UPDATE perm.roles SET display_name_de = 'Personal' WHERE id = 'dept-hr-2';
UPDATE perm.roles SET display_name_de = 'IT' WHERE id = 'dept-it';
UPDATE perm.roles SET display_name_de = 'Marketing' WHERE id = 'dept-marketing';
UPDATE perm.roles SET display_name_de = 'Entwicklung' WHERE id = 'dept-development';
UPDATE perm.roles SET display_name_de = 'Vertrieb' WHERE id = 'dept-sales';

UPDATE perm.roles SET display_name_de = 'Verkaufsmitarbeiter' WHERE id = 'sales_rep';
UPDATE perm.roles SET display_name_de = 'Verkaufsleiter' WHERE id = 'sales_supervisor';

UPDATE perm.roles SET display_name_de = 'Buchhalter' WHERE display_name = 'Accounting Officer' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'Buchhaltungsleiter' WHERE display_name = 'Accounting Supervisor' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'Buchhaltungsmanager' WHERE display_name = 'Accounting Manager' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'Finanzleiter' WHERE display_name = 'Finance Lead' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'CFO' WHERE display_name = 'CFO' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'CEO' WHERE display_name = 'CEO' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'Administrator' WHERE display_name = 'Admin' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'Vorgesetzter' WHERE display_name = 'Supervisor' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'Manager' WHERE display_name = 'Manager' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'Sachbearbeiter' WHERE display_name = 'Officer' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'Personalleiter' WHERE display_name = 'HR Manager' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'IT-Sachbearbeiter' WHERE display_name = 'IT Officer' AND kind = 'persona';
UPDATE perm.roles SET display_name_de = 'Personalsachbearbeiter' WHERE display_name = 'HR Officer' AND kind = 'persona';

COMMIT;