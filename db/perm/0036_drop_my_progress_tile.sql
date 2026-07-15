-- Folio — drop the "My Progress" tile.
--
-- Duplicate of /expense per product decision. Users hitting
-- /my-progress are now 308-redirected to /expense via next.config.ts.
-- hired_at + perm.training_courses remain (orphaned but harmless);
-- they back any future read-only HR widgets that key off tenure / training.

BEGIN;

DELETE FROM perm.tiles WHERE id = 'my-progress';

COMMIT;
