use base64::Engine;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;

use crate::config;

const ENGINE: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

#[derive(Clone)]
pub struct Identity {
    signing_key: SigningKey,
}

impl Identity {
    /// Load identity for a specific server from disk.
    pub fn load_for(server_addr: &str) -> Result<Self, String> {
        let path = config::server_identity_path(server_addr);
        if !path.exists() {
            return Err(format!(
                "Identity key not found at {}. Run `agora setup` to generate a new keypair.",
                path.display()
            ));
        }
        let content =
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read identity: {}", e))?;
        let bytes = ENGINE
            .decode(content.trim())
            .map_err(|e| format!("Failed to decode identity: {}", e))?;
        if bytes.len() != 64 {
            return Err("Invalid identity file (expected 64 bytes)".to_string());
        }
        let secret: [u8; 32] = bytes[..32].try_into().unwrap();
        let signing_key = SigningKey::from_bytes(&secret);
        Ok(Self { signing_key })
    }

    /// Generate a new identity for a specific server and save it to disk.
    pub fn generate_for(server_addr: &str) -> Result<Self, String> {
        let path = config::server_identity_path(server_addr);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let mut key_bytes = Vec::with_capacity(64);
        key_bytes.extend_from_slice(&signing_key.to_bytes());
        key_bytes.extend_from_slice(verifying_key.as_bytes());

        let encoded = ENGINE.encode(&key_bytes);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)
                .and_then(|mut f| {
                    use std::io::Write;
                    f.write_all(encoded.as_bytes())
                })
                .map_err(|e| format!("Failed to write identity: {}", e))?;
        }

        #[cfg(not(unix))]
        {
            std::fs::write(&path, &encoded)
                .map_err(|e| format!("Failed to write identity: {}", e))?;
        }

        Ok(Self { signing_key })
    }

    pub fn public_key_base64(&self) -> String {
        ENGINE.encode(self.verifying_key().as_bytes())
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Sign a request and return (timestamp, signature_base64).
    pub fn sign_request(&self, method: &str, path: &str, body: &str) -> (String, String) {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let signing_string = format!("{}\n{}\n{}\n{}", method, path, timestamp, body);
        let signature = self.signing_key.sign(signing_string.as_bytes());
        let sig_b64 = ENGINE.encode(signature.to_bytes());
        (timestamp, sig_b64)
    }

    /// Convert our ed25519 signing key to a crypto_box SecretKey for DM encryption.
    fn crypto_box_secret(&self) -> crypto_box::SecretKey {
        use sha2::Digest;
        // Hash the ed25519 secret key to get x25519 secret key bytes
        let hash = sha2::Sha512::digest(self.signing_key.as_bytes());
        let mut x25519_bytes = [0u8; 32];
        x25519_bytes.copy_from_slice(&hash[..32]);
        // Apply clamping as per RFC 7748
        x25519_bytes[0] &= 248;
        x25519_bytes[31] &= 127;
        x25519_bytes[31] |= 64;
        crypto_box::SecretKey::from(x25519_bytes)
    }

    /// Convert a base64-encoded ed25519 public key to a crypto_box PublicKey.
    fn ed25519_pub_to_crypto_box(pub_b64: &str) -> Result<crypto_box::PublicKey, String> {
        let pub_bytes = ENGINE
            .decode(pub_b64)
            .map_err(|e| format!("Failed to decode public key: {}", e))?;
        if pub_bytes.len() != 32 {
            return Err("Invalid public key length".to_string());
        }

        // Convert Edwards point to Montgomery point
        let edwards = curve25519_dalek::edwards::CompressedEdwardsY(
            pub_bytes.as_slice().try_into().unwrap(),
        );
        let point = edwards
            .decompress()
            .ok_or("Failed to decompress Edwards point")?;
        let montgomery = point.to_montgomery();

        Ok(crypto_box::PublicKey::from(montgomery.to_bytes()))
    }

    /// Encrypt a plaintext message for a recipient given their ed25519 public key (base64).
    pub fn encrypt_for(
        &self,
        recipient_pub_b64: &str,
        plaintext: &str,
    ) -> Result<(String, String), String> {
        use crypto_box::{aead::Aead, SalsaBox, Nonce};

        let our_secret = self.crypto_box_secret();
        let their_public = Self::ed25519_pub_to_crypto_box(recipient_pub_b64)?;

        let salsa_box = SalsaBox::new(&their_public, &our_secret);

        let mut nonce_bytes = [0u8; 24];
        rand::RngCore::fill_bytes(&mut OsRng, &mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext = salsa_box
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| format!("Encryption failed: {}", e))?;

        Ok((ENGINE.encode(&ciphertext), ENGINE.encode(nonce_bytes)))
    }

    /// Decrypt a message from a sender given their ed25519 public key (base64).
    pub fn decrypt_from(
        &self,
        sender_pub_b64: &str,
        ciphertext_b64: &str,
        nonce_b64: &str,
    ) -> Result<String, String> {
        use crypto_box::{aead::Aead, SalsaBox, Nonce};

        let our_secret = self.crypto_box_secret();
        let their_public = Self::ed25519_pub_to_crypto_box(sender_pub_b64)?;

        let salsa_box = SalsaBox::new(&their_public, &our_secret);

        let ciphertext = ENGINE
            .decode(ciphertext_b64)
            .map_err(|e| format!("Failed to decode ciphertext: {}", e))?;
        let nonce_bytes = ENGINE
            .decode(nonce_b64)
            .map_err(|e| format!("Failed to decode nonce: {}", e))?;

        if nonce_bytes.len() != 24 {
            return Err("Invalid nonce length".to_string());
        }

        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = salsa_box
            .decrypt(nonce, ciphertext.as_slice())
            .map_err(|_| "Decryption failed (wrong key or corrupted message)".to_string())?;

        String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8 in decrypted message: {}", e))
    }
}
