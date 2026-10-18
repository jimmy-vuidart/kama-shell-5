//! How secrets cross the bus: as they are (`plain`), or encrypted with a key
//! agreed on when the session opens (`dh-ietf1024-sha256-aes128-cbc-pkcs7`,
//! what libsecret asks for first).
//!
//! The agreement follows libsecret (`secret-session.c`, `egg-dh-libgcrypt.c`):
//! Diffie-Hellman in the 1024-bit group of RFC 2409, public keys as unsigned
//! big-endian numbers, the shared number left-padded with zeros to the size
//! of the prime, then HKDF-SHA256 without salt nor info down to an AES-128
//! key. Each secret is AES-128-CBC with PKCS#7 padding, its 16-byte IV in the
//! `parameters` of the secret.

use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use hkdf::Hkdf;
use num_bigint::BigUint;
use sha2::Sha256;

pub const PLAIN: &str = "plain";
pub const DH_AES: &str = "dh-ietf1024-sha256-aes128-cbc-pkcs7";

/// The prime of the second Oakley group (RFC 2409, section 6.2).
const PRIME: &str = "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD1\
                     29024E088A67CC74020BBEA63B139B22514A08798E3404DD\
                     EF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245\
                     E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7ED\
                     EE386BFB5A899FA5AE9F24117C4B1FE649286651ECE65381\
                     FFFFFFFFFFFFFFFF";
const GENERATOR: u32 = 2;
const PRIME_BYTES: usize = 128;
const BLOCK: usize = 16;

type Encryptor = cbc::Encryptor<aes::Aes128>;
type Decryptor = cbc::Decryptor<aes::Aes128>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error("algorithm {0:?} is not supported")]
    Unsupported(String),
    #[error("the public key of the client is not acceptable")]
    BadPublicKey,
    #[error("the secret cannot be decrypted")]
    BadSecret,
    #[error("no randomness: {0}")]
    Random(String),
}

/// How the secrets of one session are carried.
pub struct Transport {
    /// `None` for `plain`.
    key: Option<[u8; BLOCK]>,
}

impl Transport {
    /// Opens a session for `algorithm`, given what the client sent. Returns
    /// what to send back: nothing for `plain`, the public key of the
    /// keyring otherwise.
    pub fn open(algorithm: &str, input: &[u8]) -> Result<(Self, Vec<u8>), TransportError> {
        match algorithm {
            PLAIN => Ok((Self { key: None }, Vec::new())),
            DH_AES => {
                let (private, public) = keypair()?;
                let key = agree(&private, input)?;
                Ok((Self { key: Some(key) }, public))
            }
            other => Err(TransportError::Unsupported(other.to_owned())),
        }
    }

    /// The `parameters` and the `value` of a secret sent to the client.
    pub fn encrypt(&self, secret: &[u8]) -> Result<(Vec<u8>, Vec<u8>), TransportError> {
        let Some(key) = &self.key else {
            return Ok((Vec::new(), secret.to_vec()));
        };
        let mut iv = [0; BLOCK];
        random(&mut iv)?;
        let value = Encryptor::new(key.into(), &iv.into()).encrypt_padded_vec::<Pkcs7>(secret);
        Ok((iv.to_vec(), value))
    }

    /// The secret a client sent.
    pub fn decrypt(&self, parameters: &[u8], value: &[u8]) -> Result<Vec<u8>, TransportError> {
        let Some(key) = &self.key else {
            return Ok(value.to_vec());
        };
        let iv: [u8; BLOCK] = parameters
            .try_into()
            .map_err(|_| TransportError::BadSecret)?;
        if value.is_empty() || !value.len().is_multiple_of(BLOCK) {
            return Err(TransportError::BadSecret);
        }
        Decryptor::new(key.into(), &iv.into())
            .decrypt_padded_vec::<Pkcs7>(value)
            .map_err(|_| TransportError::BadSecret)
    }
}

pub fn random(buffer: &mut [u8]) -> Result<(), TransportError> {
    getrandom::fill(buffer).map_err(|error| TransportError::Random(error.to_string()))
}

fn prime() -> BigUint {
    BigUint::parse_bytes(PRIME.as_bytes(), 16).expect("the prime is written in hexadecimal")
}

/// A private exponent, and the public key that goes with it.
fn keypair() -> Result<(BigUint, Vec<u8>), TransportError> {
    let mut bytes = [0; PRIME_BYTES];
    random(&mut bytes)?;
    let prime = prime();
    // Between 2 and p - 2.
    let private = BigUint::from_bytes_be(&bytes) % (&prime - 3u32) + 2u32;
    let public = BigUint::from(GENERATOR).modpow(&private, &prime);
    Ok((private, public.to_bytes_be()))
}

/// The AES key both sides derive from their private exponent and the
/// public key of the other.
fn agree(private: &BigUint, peer: &[u8]) -> Result<[u8; BLOCK], TransportError> {
    let prime = prime();
    let peer = BigUint::from_bytes_be(peer);
    // 1 and p - 1 would give away the shared number.
    if peer <= BigUint::from(1u32) || peer >= &prime - 1u32 {
        return Err(TransportError::BadPublicKey);
    }
    let shared = peer.modpow(private, &prime).to_bytes_be();
    let mut padded = [0; PRIME_BYTES];
    padded[PRIME_BYTES - shared.len()..].copy_from_slice(&shared);
    let mut key = [0; BLOCK];
    Hkdf::<Sha256>::new(None, &padded)
        .expand(&[], &mut key)
        .expect("16 bytes is a valid length for HKDF-SHA256");
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The client side of a session, as libsecret does it.
    fn client() -> (BigUint, Vec<u8>) {
        keypair().unwrap()
    }

    #[test]
    fn both_sides_of_a_session_agree_on_the_key() {
        let (private, public) = client();

        let (transport, answer) = Transport::open(DH_AES, &public).unwrap();
        let key = agree(&private, &answer).unwrap();

        assert_eq!(transport.key, Some(key));
    }

    #[test]
    fn a_secret_goes_back_and_forth_encrypted() {
        let (private, public) = client();
        let (keyring, answer) = Transport::open(DH_AES, &public).unwrap();
        let client = Transport {
            key: Some(agree(&private, &answer).unwrap()),
        };

        let (iv, value) = client.encrypt(b"correct horse battery staple").unwrap();
        assert_eq!(iv.len(), 16);
        assert_ne!(&value[..], b"correct horse battery staple");
        assert_eq!(
            keyring.decrypt(&iv, &value).unwrap(),
            b"correct horse battery staple"
        );

        // A whole block of padding when the secret fills its blocks.
        let (iv, value) = keyring.encrypt(&[1; 16]).unwrap();
        assert_eq!(value.len(), 32);
        assert_eq!(client.decrypt(&iv, &value).unwrap(), [1; 16]);
    }

    #[test]
    fn plain_sends_the_secret_as_it_is() {
        let (transport, answer) = Transport::open(PLAIN, b"").unwrap();

        assert!(answer.is_empty());
        assert_eq!(
            transport.encrypt(b"s").unwrap(),
            (Vec::new(), b"s".to_vec())
        );
        assert_eq!(transport.decrypt(b"", b"s").unwrap(), b"s");
    }

    #[test]
    fn an_unknown_algorithm_is_refused() {
        assert!(matches!(
            Transport::open("dh-ietf1024-sha1-aes128-cbc-pkcs7", b""),
            Err(TransportError::Unsupported(_))
        ));
    }

    #[test]
    fn a_degenerate_public_key_is_refused() {
        let one = [1];
        let below_prime = (prime() - 1u32).to_bytes_be();

        assert!(matches!(
            Transport::open(DH_AES, &one),
            Err(TransportError::BadPublicKey)
        ));
        assert!(matches!(
            Transport::open(DH_AES, &below_prime),
            Err(TransportError::BadPublicKey)
        ));
    }

    #[test]
    fn a_damaged_secret_is_refused() {
        let (_, public) = client();
        let (transport, _) = Transport::open(DH_AES, &public).unwrap();

        assert_eq!(
            transport.decrypt(&[0; 8], &[0; 16]),
            Err(TransportError::BadSecret)
        );
        assert_eq!(
            transport.decrypt(&[0; 16], &[0; 15]),
            Err(TransportError::BadSecret)
        );
    }

    #[test]
    fn the_prime_has_1024_bits() {
        assert_eq!(prime().bits(), 1024);
    }
}
