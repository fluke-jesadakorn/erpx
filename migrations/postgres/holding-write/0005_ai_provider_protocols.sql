ALTER TABLE ai_connection_revisions
  ADD COLUMN provider_preset TEXT NOT NULL DEFAULT 'custom' CHECK (provider_preset IN ('custom','opencode-zen','opencode-go')),
  ADD COLUMN text_protocol TEXT NOT NULL DEFAULT 'chat-completions' CHECK (text_protocol IN ('chat-completions','responses','messages')),
  ADD COLUMN image_protocol TEXT NOT NULL DEFAULT 'chat-completions' CHECK (image_protocol IN ('chat-completions','responses','messages'));
