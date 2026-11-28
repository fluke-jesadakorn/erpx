export const socialDraftAttachmentJoin='JOIN social_post_drafts d ON d.id=a.owner_id';

export const socialDraftFilesReadSql='SELECT id,author_id,company_id,published_post_id,edit_post_id,edit_saved_version,cancelled_at,edit_post_version,comment_parent_post_id,published_comment_id FROM social_post_drafts';
