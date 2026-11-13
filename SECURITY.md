# Security Policy

## Core Principle: 100% Offline, Zero-Telemetry, Encrypted by Default

Tether is designed from the ground up for strict local-only crash recovery. It never transmits data over a network, makes zero socket calls, embeds no telemetry, and encrypts all captured text at rest.

---

## 1. Threat Model & Security Boundaries

Tether records typing so work is not lost when an application crashes, freezes, or reboots unexpectedly. Because keystroke capture is sensitive, Tether enforces strict boundaries:

### What Tether Protects Against
* **Process crashes & force-quits:** Reconstructs in-flight text via the encrypted append-only ring buffer.
* **Cold power loss & kernel panics:** Microsecond page-cache flushed ring buffer minimizes exposure to <20ms.
* **Disk inspection by unauthorized local users:** All saved drafts, ring slots, and blind indices are encrypted with AES-256-GCM and stored with private filesystem permissions (POSIX `0600` / Windows restricted user ACL).
* **Metadata leak in database:** App names and window titles are encrypted inside the ciphertext payload; database lookups use an HMAC-SHA256 blind index derived from a separate key.
* **Memory dumps after exit:** Cryptographic keys implement `Zeroize` and scrub themselves from RAM on drop.

### What Tether Deliberately Refuses to Do
* **No network access:** There is zero network code in the binary. The crate dependencies are audited to ensure no HTTP client, socket, or telemetry library is compiled.
* **No password capture:** Fields identified as password/secure fields by OS accessibility APIs (`UIAutomation` on Windows, `AXUIElement` on macOS) are dropped at the driver level before reaching disk.
* **No plain-text storage:** Neither the ring buffer (`ring.bin`) nor the SQLite database (`vault.db`) ever stores unencrypted text.

---

## 2. Cryptographic Architecture

```
                  ┌──────────────────────────────┐
                  │ Master Key (32 random bytes) │
                  └──────────────┬───────────────┘
                                 │ + 16-byte random salt
                                 ▼
                         HKDF-Expand (SHA-256)
               ┌─────────────────┼─────────────────┐
               ▼                 ▼                 ▼
          File Key           Index Key          Ring Key
        (AES-256-GCM)      (HMAC-SHA256)      (AES-256-GCM)
               │                 │                 │
               ▼                 ▼                 ▼
          Vault Drafts      Blind Index        Ring Slots
         in `vault.db`     Searchability     in `ring.bin`
```

* **Cipher:** AES-256-GCM authenticated encryption.
* **Key Separation:** Derived via HKDF-SHA256 into three dedicated subkeys:
  * `File Key`: Encrypts draft payload JSON `{app, text}` with authenticated data binding (AAD).
  * `Index Key`: Generates HMAC-SHA256 blind indices (`field_id`) for searchability without exposing app identities.
  * `Ring Key`: Encrypts each 512-byte slot in the crash buffer independently.
* **Nonce Freshness:** Every record and ring slot generates a fresh 96-bit nonce using the OS CSPRNG (`getrandom`).
* **Tamper Resistance:** GCM authentication tags verify integrity before decryption. Tampered ciphertexts fail closed and never return corrupt plaintext.

---

## 3. Real-Time Redaction & Privacy Filter

Before text reaches either RAM or disk, the policy engine runs heuristic and algorithmic sanitization:

| Sensitive Type | Detection Method | Action |
|---|---|---|
| **Passwords / PINs** | OS accessibility secure input flag | Dropped entirely (not saved) |
| **Payment Cards** | 13–19 digit sequences verified with Luhn checksum | Masked in place (`****-****-****-1234`) |
| **Private Keys** | PEM headers (`BEGIN RSA PRIVATE KEY`, `OPENSSH`, etc.) | Redacted |
| **API Tokens** | High-entropy prefix match (`ghp_`, `sk_live_`, AWS keys, JWTs) | Redacted |
| **Blacklisted Apps** | Process name matching against 25+ sensitive tools (1Password, KeePass, Bitwarden, GPG, pinentry) | Dropped entirely |

---

## 4. Desktop Shell Hardening (Tauri v2)

The desktop shell ([desktop/](desktop/)) adheres to least-privilege principles:

1. **Strict Offline CSP:**
   ```
   default-src 'self' asset: http://asset.localhost;
   connect-src ipc: http://ipc.localhost;
   img-src 'self' asset: http://asset.localhost data: blob:;
   style-src 'self';
   script-src 'self';
   object-src 'none';
   base-uri 'none';
   form-action 'none';
   ```
2. **Minimal Command Manifest:** Only `get_app_info` is registered. Filesystem, shell, dialog, and HTTP Tauri plugins are disabled.
3. **No External Assets:** All styles, scripts, and fonts are bundled locally. Zero CDN dependencies.
4. **Diagnostic Sanitization:** Bridge error logs only output high-level static error labels (e.g. `TETHER_APP_STARTUP_FAILED`). Secret sentinels, keys, and drafts are never output to logs.

---

## 5. Supported Versions

| Version | Supported |
|---|---|
| 0.1.x | :white_check_mark: |
| < 0.1.0 | :x: |

---

## 6. Reporting a Vulnerability

If you discover a security vulnerability in Tether:

1. **Do not open a public issue.**
2. Report privately via [GitHub Security Advisories](https://github.com/AMG555/Tether/security/advisories/new) or email `security@tether.dev`.
3. Provide:
   * Description of the vulnerability.
   * Steps or script to reproduce.
   * Affected operating systems and versions.
4. We acknowledge reports within 48 hours and provide patches within 7 business days for verified critical vulnerabilities.
