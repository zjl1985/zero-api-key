use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

const ITERATIONS: u32 = 210_000;
const SALT_LEN: usize = 16;
const HASH_LEN: usize = 32;

pub fn hash_password(password: &str) -> String {
    let salt: [u8; SALT_LEN] = rand::random();
    let mut out = [0u8; HASH_LEN];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, ITERATIONS, &mut out);
    format!("{}${}", B64.encode(salt), B64.encode(out))
}

pub fn verify_password(stored: &str, password: &str) -> bool {
    let mut parts = stored.split('$');
    let (Some(salt), Some(expected), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let (Ok(salt), Ok(expected)) = (B64.decode(salt), B64.decode(expected)) else {
        return false;
    };
    let mut out = [0u8; HASH_LEN];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, ITERATIONS, &mut out);
    out[..] == expected[..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let hash = hash_password("s3cret");
        assert!(verify_password(&hash, "s3cret"));
        assert!(!verify_password(&hash, "wrong"));
        assert!(!verify_password(&hash, ""));
    }

    #[test]
    fn unique_salts() {
        assert_ne!(hash_password("same"), hash_password("same"));
    }

    #[test]
    fn malformed_stored_hash_never_matches() {
        assert!(!verify_password("garbage", "x"));
        assert!(!verify_password("", "x"));
        assert!(!verify_password("!!!$!!!", "x"));
        assert!(!verify_password("a$b$c", "x"));
    }
}
