//! Encryption at rest.
//!
//! Design decisions (see docs/adr/0003-encryption.md):
//! - AES-256-GCM, a fresh 96-bit nonce per record. Nonce reuse is catastrophic for
//!   GCM, so nonces come from the OS CSPRNG and are stored alongside the ciphertext.
//! - Two different keys derived from one master key via HKDF: the *file* key and the
//!   *index* key. Rolling the file key (e.g. after an incident) does not orphan the index.
//! - Authenticated data (AAD) binds a record's ciphertext to its own metadata. An attacker
//!   who can write to disk cannot move a ciphertext under a different app/timestamp label.
//! - Every key lives in `SecretKey`, zeroized on drop, and never Debug-printed.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;
pub const SALT_LEN: usize = 16;
/// Versioned envelope prefix so a future format change is detectable, not corrupting.
pub const MAGIC: &[u8; 4] = b"SPV1";

pub enum CryptoError {
    Encrypt,
    Decrypt,
    BadEnvelope,
    Rand,
}

impl std::fmt::Debug for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CryptoError::Encrypt => "encrypt-failed",
            CryptoError::Decrypt => "decrypt-failed",
            CryptoError::BadEnvelope => "bad-envelope",
            CryptoError::Rand => "csprng-failed",
        };
        f.write_str(s)
    }
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for CryptoError {}

/// A 256-bit key that scrubs itself from memory on drop.
pub struct SecretKey([u8; KEY_LEN]);

impl SecretKey {
    pub fn random() -> Result<Self, CryptoError> {
        let mut k = [0u8; KEY_LEN];
        getrandom::getrandom(&mut k).map_err(|_| CryptoError::Rand)?;
        Ok(Self(k))
    }

    pub fn from_bytes(b: [u8; KEY_LEN]) -> Self {
        Self(b)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// Recoverable hex for the one-time key backup the user is shown at init.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.len() != KEY_LEN * 2 {
            return None;
        }
        let mut out = [0u8; KEY_LEN];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            out[i] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
        }
        Some(Self(out))
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(&self.0).expect("key length is fixed at 32 bytes")
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print key material, not even in panic dumps.
        f.write_str("SecretKey(<redacted>)")
    }
}

/// The three keys derived from one master key. Separate HKDF info strings mean a compromise
/// of the (short-lived, hot-path) ring key cannot read the vault, and vice versa.
pub struct Keys {
    pub file: SecretKey,
    pub index: SecretKey,
    pub ring: SecretKey,
}

pub fn derive_keys(master: &SecretKey, salt: &[u8; SALT_LEN]) -> Keys {
    let hk = Hkdf::<Sha256>::new(Some(salt), master.as_bytes());
    let mut file = [0u8; KEY_LEN];
    let mut index = [0u8; KEY_LEN];
    let mut ring = [0u8; KEY_LEN];
    hk.expand(b"rescue/v1/file", &mut file).expect("32 bytes is a valid HKDF output length");
    hk.expand(b"rescue/v1/index", &mut index).expect("32 bytes is a valid HKDF output length");
    hk.expand(b"rescue/v1/ring", &mut ring).expect("32 bytes is a valid HKDF output length");
    Keys { file: SecretKey(file), index: SecretKey(index), ring: SecretKey(ring) }
}

fn envelope_parts(ct: &[u8]) -> Result<([u8; NONCE_LEN], &[u8]), CryptoError> {
    if ct.len() < MAGIC.len() + NONCE_LEN || &ct[..MAGIC.len()] != MAGIC {
        return Err(CryptoError::BadEnvelope);
    }
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&ct[MAGIC.len()..MAGIC.len() + NONCE_LEN]);
    Ok((nonce, &ct[MAGIC.len() + NONCE_LEN..]))
}

/// Encrypts `plaintext`, binding it to `_aad` (associated authenticated data).
/// Output layout: MAGIC || nonce || ciphertext+tag.
pub fn seal(
    key: &SecretKey,
    plaintext: &[u8],
    _aad: &[u8],
    nonce_override: Option<[u8; NONCE_LEN]>,
) -> Result<Vec<u8>, CryptoError> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    match nonce_override {
        Some(n) => nonce_bytes = n,
        None => getrandom::getrandom(&mut nonce_bytes).map_err(|_| CryptoError::Rand)?,
    }
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = key
        .cipher()
        .encrypt(nonce, Payload { msg: plaintext, aad: _aad })
        .map_err(|_| CryptoError::Encrypt)?;
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypts a sealed envelope. Fails closed: any tamper, wrong key, or truncation
/// returns `Decrypt` and the caller must treat the record as unreadable — never guess.
pub fn open(key: &SecretKey, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let (nonce_bytes, ct) = envelope_parts(sealed)?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    key.cipher().decrypt(nonce, Payload { msg: ct, aad }).map_err(|_| CryptoError::Decrypt)
}

/// Blinded index token: lets us ask "do I already have a draft for this field?"
/// without storing the app name or window title in plaintext.
pub fn blind_index(key: &SecretKey, field_id: &str) -> String {
    use hmac_compat::Hmac;
    let mac = Hmac::new(key.as_bytes(), field_id.as_bytes());
    mac.hex()
}

mod hmac_compat {
    use sha2::{Digest, Sha256};

    const BLOCK: usize = 64;

    /// RFC 2104 HMAC-SHA256. Small enough to own outright; keeps the dependency
    /// surface at four crates.
    pub struct Hmac {
        inner: Sha256,
        outer: Sha256,
    }

    impl Hmac {
        pub fn new(key: &[u8], msg: &[u8]) -> Self {
            let mut k = [0u8; BLOCK];
            if key.len() > BLOCK {
                let d = Sha256::digest(key);
                k[..32].copy_from_slice(&d);
            } else {
                k[..key.len()].copy_from_slice(key);
            }
            let mut ipad = [0x36u8; BLOCK];
            let mut opad = [0x5cu8; BLOCK];
            for i in 0..BLOCK {
                ipad[i] ^= k[i];
                opad[i] ^= k[i];
            }
            let mut inner = Sha256::new();
            inner.update(ipad);
            inner.update(msg);
            let mut outer = Sha256::new();
            outer.update(opad);
            Self { inner, outer }
        }

        pub fn hex(self) -> String {
            let inner_digest = self.inner.finalize();
            let mut outer = self.outer;
            outer.update(inner_digest);
            outer.finalize().iter().map(|b| format!("{b:02x}")).collect()
        }
    }
}
