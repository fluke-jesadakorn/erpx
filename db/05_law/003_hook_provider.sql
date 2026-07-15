INSERT INTO hook.hook_providers (id, display_name, kind, secret_env)
VALUES ('line_law', 'Law LINE OA', 'line', 'LINE_CHANNEL_SECRET')
ON CONFLICT (id) DO NOTHING;
