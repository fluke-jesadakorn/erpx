export const homeReadCursorProjection="COALESCE((SELECT read_through FROM home_read_cursors WHERE company_id=? AND user_id=? AND workspace_base=?),0)";
