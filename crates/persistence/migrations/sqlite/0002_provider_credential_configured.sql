-- credential_configured 落库：Admin API 列表不再逐次读取 SecretStore（macOS Keychain 读取需要授权弹窗）。
ALTER TABLE providers ADD COLUMN credential_configured INTEGER NOT NULL DEFAULT 0;
