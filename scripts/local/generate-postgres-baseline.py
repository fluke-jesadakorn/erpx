"""One-time, offline port of the final archived SQLite schema. Never used at runtime."""
from pathlib import Path
import re
import sqlite3

root = Path(__file__).resolve().parents[2]
helpers = """-- Shared PostgreSQL functions used by the application-owned SQL contract.
CREATE FUNCTION erpx_json_valid(document text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$ SELECT document IS JSON $$;
CREATE FUNCTION erpx_json_extract(document text, path text) RETURNS text LANGUAGE sql IMMUTABLE AS $$
 SELECT jsonb_path_query_first(document::jsonb,path::jsonpath) #>> '{}' $$;
CREATE FUNCTION erpx_json_each(document text, path text DEFAULT '$') RETURNS TABLE(key text,value text) LANGUAGE sql IMMUTABLE AS $$
 SELECT (ordinality-1)::text, element #>> '{}' FROM jsonb_array_elements(COALESCE(jsonb_path_query_first(document::jsonb,path::jsonpath),'[]'::jsonb)) WITH ORDINALITY AS a(element,ordinality) $$;
CREATE FUNCTION erpx_json(document text) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT document::jsonb $$;
CREATE FUNCTION erpx_json(document jsonb) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT document $$;
CREATE FUNCTION erpx_json_array_append(state jsonb, element anyelement) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT state || jsonb_build_array(element) $$;
CREATE AGGREGATE erpx_json_array_agg(anyelement) (SFUNC=erpx_json_array_append,STYPE=jsonb,INITCOND='[]');
"""
def dialect(sql):
    sql=re.sub(r'\)\s*STRICT\s*$',')',sql.strip())
    sql=re.sub(r'\bINTEGER\b','BIGINT',sql,flags=re.I)
    sql=re.sub(r'\bREAL\b','DOUBLE PRECISION',sql,flags=re.I)
    sql=re.sub(r'\bBLOB\b','BYTEA',sql,flags=re.I)
    sql=sql.replace('json_valid(', 'erpx_json_valid(')
    sql=re.sub(r'json_type\((\w+)\)',r'jsonb_typeof(\1::jsonb)',sql)
    sql=re.sub(r'json_array_length\((\w+)\)',r'jsonb_array_length(\1::jsonb)',sql)
    # Preserve the mixed-case Better Auth column names and every quoted literal.
    return re.sub(r"'(?:''|[^'])*'|\"(?:\"\"|[^\"])*\"|\b[a-z_][A-Za-z0-9_]*\b",lambda m: '"'+m[0]+'"' if not m[0].startswith(('"',"'")) and re.search('[A-Z]',m[0]) else m[0],sql)
for track in ['control','control-read','holding-write','holding-read']:
    db=sqlite3.connect(':memory:')
    inputs=sorted((root/'migrations'/track).glob('*.sql'))
    for file in inputs: db.executescript(file.read_text())
    out=['-- PostgreSQL 18 baseline; fresh installations only. No demo data.\n-- Source: final schema of '+track+' through '+inputs[-1].name+'\n',helpers]
    foreign=[]
    reference=r'REFERENCES\s+(?:"[^"]+"|\w+)\s*\([^)]*\)(?:\s+ON\s+(?:DELETE|UPDATE)\s+(?:CASCADE|SET NULL|RESTRICT|NO ACTION|SET DEFAULT))*'
    for name,sql in db.execute("SELECT name,sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY rowid"):
        sql=re.sub(r',\s*FOREIGN KEY\s*\([^)]*\)\s*'+reference,'',sql,flags=re.I)
        sql=re.sub(reference,'',sql,flags=re.I)
        out.append(dialect(sql)+';')
        fks={}
        for id,seq,target,col,to,on_update,on_delete,match in db.execute('PRAGMA foreign_key_list("'+name+'")'):
            fks.setdefault(id,{'target':target,'from':[],'to':[],'update':on_update,'delete':on_delete})
            fks[id]['from'].append('"'+col+'"'); fks[id]['to'].append('"'+to+'"')
        for id,fk in fks.items():
            foreign.append(f'ALTER TABLE "{name}" ADD CONSTRAINT "{name}_fk_{id}" FOREIGN KEY ({",".join(fk["from"])}) REFERENCES "{fk["target"]}" ({",".join(fk["to"])}) ON DELETE {fk["delete"]} ON UPDATE {fk["update"]};')
    out.extend(foreign)
    for name,sql in db.execute("SELECT name,sql FROM sqlite_master WHERE type='index' AND sql IS NOT NULL ORDER BY rowid"):
        out.append(dialect(sql)+';')
    for name,table,sql in db.execute("SELECT name,tbl_name,sql FROM sqlite_master WHERE type='trigger'"):
        if 'RAISE(ABORT' in sql:
            predicate=sql[sql.index('WHEN ')+5:sql.index('BEGIN')].strip().replace(' IS NOT OLD.',' IS DISTINCT FROM OLD.')
            message=re.search(r"RAISE\(ABORT,\s*'([^']+)'",sql)[1]
            timing=sql[sql.index('BEFORE'):sql.index(' ON ')].strip()
            out.append(f"CREATE FUNCTION {name}_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF {predicate} THEN RAISE EXCEPTION '{message}' USING ERRCODE='23514'; END IF; RETURN NEW; END $$;\nCREATE TRIGGER {name} {timing} ON {table} FOR EACH ROW EXECUTE FUNCTION {name}_fn();")
    if track=='holding-write':
        out.append("""CREATE FUNCTION social_comment_revision_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' THEN NEW.version=OLD.version+1; END IF;
 UPDATE social_posts SET version=version+1,updated_at=GREATEST(updated_at,NEW.updated_at) WHERE id=NEW.post_id;
 RETURN NEW;
END $$;
CREATE TRIGGER social_comment_insert_revision BEFORE INSERT ON social_comments FOR EACH ROW EXECUTE FUNCTION social_comment_revision_fn();
CREATE TRIGGER social_comment_update_revision BEFORE UPDATE OF body,state ON social_comments FOR EACH ROW EXECUTE FUNCTION social_comment_revision_fn();
ALTER TABLE outbox_events ADD COLUMN processed_at BIGINT;
ALTER TABLE outbox_events ADD COLUMN next_attempt_at BIGINT NOT NULL DEFAULT 0;
ALTER TABLE outbox_events ADD COLUMN lease_token TEXT;
ALTER TABLE outbox_events ADD COLUMN last_error TEXT;
ALTER TABLE outbox_events ADD COLUMN dead_letter_at BIGINT;
CREATE INDEX outbox_pending_jobs ON outbox_events(next_attempt_at,created_at) WHERE processed_at IS NULL AND dead_letter_at IS NULL;
""")
    if track=='control':
        out.append('CREATE UNIQUE INDEX user_email_normalized ON "user" (lower(email));')
        out.append("CREATE TABLE runtime_health(service TEXT PRIMARY KEY,heartbeat_at BIGINT NOT NULL,details_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(details_json))); ")
    directory=root/'migrations'/'postgres'/track
    directory.mkdir(parents=True,exist_ok=True)
    (directory/'0001_baseline.sql').write_text('\n\n'.join(out)+'\n')
    views=[dialect(sql)+';' for (sql,) in db.execute("SELECT sql FROM sqlite_master WHERE type='view' ORDER BY rowid")]
    if views:
        (directory/'0003_views.sql').write_text('-- Final read views from the archived schema.\n'+ '\n\n'.join(views)+'\n')
    print(track,len(list(db.execute("SELECT 1 FROM sqlite_master WHERE type='table'"))),'tables')
