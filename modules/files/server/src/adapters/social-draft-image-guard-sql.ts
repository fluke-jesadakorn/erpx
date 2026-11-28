/** Byte-identical social-draft image EXISTS guard prefix shared by the
 * collaboration-home retained/uploaded/comment guards. Text only: every call
 * site renders its own full statement with its own binds; no SQL text is
 * accepted from a caller and no result is cached here. */
export const socialDraftImageExistsSql = 'EXISTS(SELECT 1 FROM attachments a JOIN ';
