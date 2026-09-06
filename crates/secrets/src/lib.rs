// Copyright 2026 YONGZHE CHEN
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! SecretStore Port（方案 §21.3）：Desktop 走 OS Keychain，Server 走 Env/内存；
//! Secret 值实现防 Debug 明文输出（方案 §41.1）。

use async_trait::async_trait;
use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("keyring backend error: {0}")]
    Keyring(String),
    #[error("secret not found: {0}")]
    NotFound(String),
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretValue(***)")
    }
}

impl fmt::Display for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "***")
    }
}

/// Secret 引用，形如 `provider/{provider_id}/api_key`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SecretRef(pub String);

impl SecretRef {
    pub fn provider_key(provider_id: &str) -> Self {
        Self(format!("provider/{provider_id}/api_key"))
    }
}

#[async_trait]
pub trait SecretStore: Send + Sync {
    async fn get(&self, key: &SecretRef) -> Result<Option<SecretValue>, SecretError>;
    async fn set(&self, key: &SecretRef, value: SecretValue) -> Result<(), SecretError>;
    async fn delete(&self, key: &SecretRef) -> Result<(), SecretError>;
}

/// 进程内存储，用于测试与 Keychain 不可用时的兜底。
#[derive(Default)]
pub struct MemorySecretStore {
    entries: tokio::sync::RwLock<std::collections::HashMap<SecretRef, SecretValue>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SecretStore for MemorySecretStore {
    async fn get(&self, key: &SecretRef) -> Result<Option<SecretValue>, SecretError> {
        Ok(self.entries.read().await.get(key).cloned())
    }

    async fn set(&self, key: &SecretRef, value: SecretValue) -> Result<(), SecretError> {
        self.entries.write().await.insert(key.clone(), value);
        Ok(())
    }

    async fn delete(&self, key: &SecretRef) -> Result<(), SecretError> {
        self.entries.write().await.remove(key);
        Ok(())
    }
}

/// OS Keychain（macOS Keychain / Windows Credential Manager / Linux Secret Service）。
pub struct KeyringSecretStore {
    service: String,
}

impl KeyringSecretStore {
    pub fn new() -> Self {
        Self {
            service: "aihub".to_string(),
        }
    }

    fn entry(&self, key: &SecretRef) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, &key.0).map_err(|e| SecretError::Keyring(e.to_string()))
    }
}

impl Default for KeyringSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretStore for KeyringSecretStore {
    async fn get(&self, key: &SecretRef) -> Result<Option<SecretValue>, SecretError> {
        let entry = self.entry(key)?;
        // keyring 为同步 API，避免在 async 上下文阻塞，放入阻塞线程池。
        let result = tokio::task::spawn_blocking(move || entry.get_password())
            .await
            .map_err(|e| SecretError::Keyring(format!("join error: {e}")))?;
        match result {
            Ok(password) => Ok(Some(SecretValue::new(password))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Keyring(e.to_string())),
        }
    }

    async fn set(&self, key: &SecretRef, value: SecretValue) -> Result<(), SecretError> {
        let entry = self.entry(key)?;
        let password = value.0;
        tokio::task::spawn_blocking(move || entry.set_password(&password))
            .await
            .map_err(|e| SecretError::Keyring(format!("join error: {e}")))?
            .map_err(|e| SecretError::Keyring(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, key: &SecretRef) -> Result<(), SecretError> {
        let entry = self.entry(key)?;
        tokio::task::spawn_blocking(move || entry.delete_credential())
            .await
            .map_err(|e| SecretError::Keyring(format!("join error: {e}")))?
            .map_err(|e| match e {
                keyring::Error::NoEntry => SecretError::NotFound(key.0.clone()),
                other => SecretError::Keyring(other.to_string()),
            })?;
        Ok(())
    }
}

/// 环境变量存储（Server 模式，方案 §21.3）：仅读取 `env/<VAR_NAME>` 形式的引用。
pub struct EnvSecretStore;

impl EnvSecretStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EnvSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretStore for EnvSecretStore {
    async fn get(&self, key: &SecretRef) -> Result<Option<SecretValue>, SecretError> {
        let Some(name) = key.0.strip_prefix("env/") else {
            return Ok(None);
        };
        Ok(std::env::var(name).ok().map(SecretValue::new))
    }

    async fn set(&self, _key: &SecretRef, _value: SecretValue) -> Result<(), SecretError> {
        Err(SecretError::NotFound("env store is read-only".into()))
    }

    async fn delete(&self, _key: &SecretRef) -> Result<(), SecretError> {
        Ok(())
    }
}

/// 按顺序尝试多个 store；读返回第一个命中，写写入全部成功者中的第一个。
pub struct CompositeSecretStore {
    stores: Vec<Box<dyn SecretStore>>,
}

impl CompositeSecretStore {
    pub fn new(stores: Vec<Box<dyn SecretStore>>) -> Self {
        Self { stores }
    }
}

#[async_trait]
impl SecretStore for CompositeSecretStore {
    async fn get(&self, key: &SecretRef) -> Result<Option<SecretValue>, SecretError> {
        for store in &self.stores {
            match store.get(key).await {
                Ok(Some(value)) => return Ok(Some(value)),
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(target: "aihub::secrets", key = %key.0, error = %e, "secret store read failed, trying next");
                }
            }
        }
        Ok(None)
    }

    async fn set(&self, key: &SecretRef, value: SecretValue) -> Result<(), SecretError> {
        let mut last_err = None;
        for store in &self.stores {
            match store.set(key, value.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or(SecretError::NotFound("no store available".into())))
    }

    async fn delete(&self, key: &SecretRef) -> Result<(), SecretError> {
        let mut last_err = None;
        for store in &self.stores {
            match store.delete(key).await {
                Ok(()) => return Ok(()),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or(SecretError::NotFound("no store available".into())))
    }
}

pub fn default_store() -> Box<dyn SecretStore> {
    Box::new(CompositeSecretStore::new(vec![
        Box::new(KeyringSecretStore::new()),
        Box::new(MemorySecretStore::new()),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_store_roundtrip() {
        let store = MemorySecretStore::new();
        let key = SecretRef::provider_key("p1");
        assert!(store.get(&key).await.unwrap().is_none());
        store.set(&key, SecretValue::new("sk-test")).await.unwrap();
        assert_eq!(store.get(&key).await.unwrap().unwrap().expose(), "sk-test");
        store.delete(&key).await.unwrap();
        assert!(store.get(&key).await.unwrap().is_none());
    }

    #[test]
    fn secret_value_debug_is_redacted() {
        let secret = SecretValue::new("super-secret-value");
        assert!(!format!("{secret:?}").contains("super-secret-value"));
        assert!(!format!("{secret}").contains("super-secret-value"));
    }
}
