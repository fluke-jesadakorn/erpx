ALTER TABLE organization_positions ADD COLUMN roster_required INTEGER NOT NULL DEFAULT 0 CHECK (roster_required IN (0,1));

-- Preserve live scheduling during the rollout. HR can subsequently mark these
-- positions backoffice after reviewing their holders.
UPDATE organization_positions p SET roster_required=1
WHERE EXISTS (
  SELECT 1 FROM hr_engagements e
  JOIN people_roster_assignments a ON a.engagement_id=e.id AND a.state='assigned'
  JOIN people_roster_shifts s ON s.id=a.shift_id AND s.state='active'
  WHERE e.company_id=p.company_id AND e.position_id=p.id AND s.starts_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint
);
