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

//! Application API Key：格式 `aih_live_<prefix>_<secret>`（方案 §14.4）。
//! 明文仅创建时返回一次；库中只存 sha256 hash 与 prefix（§9.7）。

use rand::Rng;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEnvironment {
    Live,
    Test,
}

impl KeyEnvironment {
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyEnvironment::Live => "aih_live",
            KeyEnvironment::Test => "aih_test",
        }
    }
}

const PREFIX_LEN: usize = 8;
const SECRET_LEN: usize = 24;

fn random_alnum(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

pub fn generate_key(environment: KeyEnvironment) -> (String, String) {
    let prefix = random_alnum(PREFIX_LEN);
    let secret = random_alnum(SECRET_LEN);
    let plaintext = format!("{}_{}_{}", environment.as_str(), prefix, secret);
    (plaintext, prefix)
}

pub fn hash_key(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

/// 从明文 key 提取可查询的 prefix；非法格式返回 None。
pub fn key_prefix(plaintext: &str) -> Option<&str> {
    let parts: Vec<&str> = plaintext.split('_').collect();
    if parts.len() != 4 {
        return None;
    }
    if parts[0] != "aih" || (parts[1] != "live" && parts[1] != "test") {
        return None;
    }
    if parts[2].is_empty() || parts[3].is_empty() {
        return None;
    }
    Some(parts[2])
}

/// 常数时间 hash 比较。
pub fn verify_hash(plaintext: &str, stored_hash: &str) -> bool {
    let computed = hash_key(plaintext);
    computed.as_bytes().ct_eq(stored_hash.as_bytes()).into()
}

pub fn masked(prefix: &str) -> String {
    format!("aih_live_{prefix}_***")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_has_expected_format() {
        let (plaintext, prefix) = generate_key(KeyEnvironment::Live);
        assert!(plaintext.starts_with("aih_live_"));
        assert_eq!(key_prefix(&plaintext), Some(prefix.as_str()));
        assert_eq!(prefix.len(), 8);
        // secret 段长度
        let parts: Vec<&str> = plaintext.split('_').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[3].len(), 24);
    }

    #[test]
    fn hash_is_stable_and_verification_constant_time() {
        let (plaintext, _) = generate_key(KeyEnvironment::Test);
        let hash = hash_key(&plaintext);
        assert_eq!(hash.len(), 64);
        assert!(verify_hash(&plaintext, &hash));
        assert!(!verify_hash(&format!("{plaintext}x"), &hash));
    }

    #[test]
    fn invalid_formats_rejected() {
        assert!(key_prefix("sk-abc").is_none());
        assert!(key_prefix("aih_wrong_abcd_1234").is_none());
        assert!(key_prefix("aih_live_").is_none());
    }
}
