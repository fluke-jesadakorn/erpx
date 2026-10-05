# Atomic secondary book projection at the posting boundary

Company document owners currently post journals through several services. A deferred PostgreSQL projection therefore creates the mapped secondary journals and their financial events in the source transaction, rather than relying on every owner to call a new application adapter. A missing account mapping aborts the entire source command. Book activation starts with an independently reviewed opening and never rewrites historical posted evidence.

The initial projection preserves the primary functional currency and original carrying amounts, including settlement FX adjustments. Secondary books with a different functional currency require a carrying-value adapter before activation; converting each settlement at its current rate would produce incorrect control balances. Recognition-policy differences are represented by separately owned book adjustments, rather than changing a business document or mutating an issued journal.
