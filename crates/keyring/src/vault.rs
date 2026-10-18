//! The keyring on disk: `keyring.bin`, encrypted with AES-256-GCM, whose key
//! is sealed in `keyring.key` by `systemd-creds --user`, with the TPM and the
//! key of this machine, for this user. Nothing is asked: the key is unsealed
//! when the keyring starts, and the file cannot be read on another machine.
//!
//! What cannot be read is never overwritten: a keyring whose key is lost or
//! wrong refuses to start rather than start empty.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use tracing::info;

use crate::store::Saved;
use crate::transport::random;

const SECRETS: &str = "keyring.bin";
const SEALED_KEY: &str = "keyring.key";
/// The name the key is sealed under: unsealing checks it.
const CREDENTIAL: &str = "coquille-keyring";
/// The start of the file, also authenticated with what follows.
const MAGIC: &[u8] = b"coquille-keyring 1\n";
const NONCE: usize = 12;
const KEY: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("{0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("systemd-creds failed: {0}")]
    Credential(String),
    #[error("{0} exists but its key {1} is missing: refusing to start rather than lose it")]
    KeyLost(PathBuf, PathBuf),
    #[error("the key does not open {0}, or the file is damaged")]
    WrongKey(PathBuf),
    #[error("{0} is not a keyring of coquille")]
    NotAKeyring(PathBuf),
    #[error("the content of {0} cannot be read: {1}")]
    Content(PathBuf, serde_json::Error),
    #[error("no randomness: {0}")]
    Random(String),
}

/// Where the key comes from.
pub enum KeySource {
    /// Sealed by systemd-creds in the directory of the keyring.
    Sealed,
    /// Raw, in a file: for trials only. Made if it does not exist.
    File(PathBuf),
}

pub struct Vault {
    path: PathBuf,
    cipher: Aes256Gcm,
}

impl Vault {
    /// Gets the key of the keyring in `directory`, making and sealing one
    /// the first time.
    pub fn open(directory: &Path, source: KeySource) -> Result<Self, VaultError> {
        create_private_dir(directory)?;
        let path = directory.join(SECRETS);
        let key = match source {
            KeySource::Sealed => sealed_key(directory, &path)?,
            KeySource::File(file) => raw_key(&file)?,
        };
        let cipher = Aes256Gcm::new_from_slice(&key).expect("the key has 32 bytes");
        Ok(Self { path, cipher })
    }

    /// What was saved, or `None` the first time.
    pub fn load(&self) -> Result<Option<Saved>, VaultError> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(VaultError::Io(self.path.clone(), error)),
        };
        let content = bytes
            .strip_prefix(MAGIC)
            .filter(|rest| rest.len() >= NONCE)
            .ok_or_else(|| VaultError::NotAKeyring(self.path.clone()))?;
        let (nonce, sealed) = content.split_at(NONCE);
        let nonce = Nonce::try_from(nonce).expect("the nonce has 12 bytes");
        let payload = Payload {
            msg: sealed,
            aad: MAGIC,
        };
        let plain = self
            .cipher
            .decrypt(&nonce, payload)
            .map_err(|_| VaultError::WrongKey(self.path.clone()))?;
        serde_json::from_slice(&plain)
            .map(Some)
            .map_err(|error| VaultError::Content(self.path.clone(), error))
    }

    /// Writes the keyring, replacing the file in one step so that a crash in
    /// the middle cannot leave half of it.
    pub fn save(&self, saved: &Saved) -> Result<(), VaultError> {
        let plain = serde_json::to_vec(saved).expect("the keyring is always serializable");
        let mut nonce = [0; NONCE];
        random(&mut nonce).map_err(|error| VaultError::Random(error.to_string()))?;
        let payload = Payload {
            msg: &plain,
            aad: MAGIC,
        };
        let sealed = self
            .cipher
            .encrypt(&Nonce::from(nonce), payload)
            .expect("AES-GCM encrypts any message of this size");
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&nonce);
        bytes.extend_from_slice(&sealed);
        write_private(&self.path, &bytes)
    }
}

/// Unseals the key, or makes and seals one when there is no keyring yet.
fn sealed_key(directory: &Path, secrets: &Path) -> Result<[u8; KEY], VaultError> {
    let sealed = directory.join(SEALED_KEY);
    if sealed.exists() {
        let output = run(
            Command::new("systemd-creds")
                .args(["decrypt", "--user", &format!("--name={CREDENTIAL}")])
                .arg(&sealed)
                .arg("-"),
            None,
        )?;
        return output
            .try_into()
            .map_err(|_| VaultError::Credential("the unsealed key has not 32 bytes".to_owned()));
    }
    if secrets.exists() {
        return Err(VaultError::KeyLost(secrets.to_owned(), sealed));
    }
    let mut key = [0; KEY];
    random(&mut key).map_err(|error| VaultError::Random(error.to_string()))?;
    let draft = sealed.with_extension("key.tmp");
    run(
        Command::new("systemd-creds")
            .args(["encrypt", "--user", &format!("--name={CREDENTIAL}"), "-"])
            .arg(&draft),
        Some(&key),
    )?;
    // Sealed or not, it is nobody else's business.
    std::fs::set_permissions(&draft, std::fs::Permissions::from_mode(0o600))
        .and_then(|()| std::fs::rename(&draft, &sealed))
        .map_err(|error| VaultError::Io(sealed.clone(), error))?;
    info!(path = %sealed.display(), "a new key was made and sealed for the keyring");
    Ok(key)
}

fn raw_key(file: &Path) -> Result<[u8; KEY], VaultError> {
    match std::fs::read(file) {
        Ok(bytes) => bytes
            .try_into()
            .map_err(|_| VaultError::Credential(format!("{} has not 32 bytes", file.display()))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut key = [0; KEY];
            random(&mut key).map_err(|error| VaultError::Random(error.to_string()))?;
            write_private(file, &key)?;
            Ok(key)
        }
        Err(error) => Err(VaultError::Io(file.to_owned(), error)),
    }
}

/// Runs `command`, feeding it `input`, and returns what it printed.
fn run(command: &mut Command, input: Option<&[u8]>) -> Result<Vec<u8>, VaultError> {
    let failed = |error: io::Error| VaultError::Credential(error.to_string());
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(failed)?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(input).map_err(failed)?;
    }
    let output = child.wait_with_output().map_err(failed)?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        return Err(VaultError::Credential(format!(
            "{}: {}",
            output.status,
            said.trim()
        )));
    }
    Ok(output.stdout)
}

fn create_private_dir(directory: &Path) -> Result<(), VaultError> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(|error| VaultError::Io(directory.to_owned(), error))
}

/// Writes a file only its owner can read, through a draft renamed at the
/// end.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), VaultError> {
    let failed = |error| VaultError::Io(path.to_owned(), error);
    let mut draft = path.as_os_str().to_owned();
    draft.push(".tmp");
    let draft = PathBuf::from(draft);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&draft)
        .map_err(failed)?;
    file.write_all(bytes).map_err(failed)?;
    file.sync_all().map_err(failed)?;
    std::fs::rename(&draft, path).map_err(failed)
}

#[cfg(test)]
mod tests {
    use crate::store::{Keyring, LOGIN, NewItem};

    use super::*;

    /// A fresh directory under the system's temporary one.
    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "coquille-keyring-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    fn some_keyring() -> Saved {
        let mut keyring = Keyring::new(None, 1);
        let item = NewItem {
            label: "Chrome Safe Storage".to_owned(),
            attributes: [("application".to_owned(), "chrome".to_owned())].into(),
            secret: b"hunter2".to_vec(),
            content_type: "text/plain".to_owned(),
        };
        keyring.create_item(LOGIN, item, false, 2).unwrap();
        keyring.saved()
    }

    #[test]
    fn a_saved_keyring_is_read_back() {
        let directory = scratch("read-back");
        let key = directory.join("key");
        let vault = Vault::open(&directory, KeySource::File(key.clone())).unwrap();
        assert!(vault.load().unwrap().is_none());

        vault.save(&some_keyring()).unwrap();
        let again = Vault::open(&directory, KeySource::File(key)).unwrap();

        assert_eq!(again.load().unwrap(), Some(some_keyring()));
    }

    #[test]
    fn the_file_is_encrypted_and_only_its_owner_can_read_it() {
        let directory = scratch("private");
        let vault = Vault::open(&directory, KeySource::File(directory.join("key"))).unwrap();

        vault.save(&some_keyring()).unwrap();
        let bytes = std::fs::read(directory.join(SECRETS)).unwrap();
        let mode = std::fs::metadata(directory.join(SECRETS))
            .unwrap()
            .permissions()
            .mode();

        assert!(!bytes.windows(7).any(|window| window == b"hunter2"));
        assert!(!bytes.windows(6).any(|window| window == b"chrome"));
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn another_key_cannot_open_the_keyring_and_leaves_it_untouched() {
        let directory = scratch("wrong-key");
        let vault = Vault::open(&directory, KeySource::File(directory.join("key"))).unwrap();
        vault.save(&some_keyring()).unwrap();
        let before = std::fs::read(directory.join(SECRETS)).unwrap();

        let other = Vault::open(&directory, KeySource::File(directory.join("other"))).unwrap();

        assert!(matches!(other.load(), Err(VaultError::WrongKey(_))));
        assert_eq!(std::fs::read(directory.join(SECRETS)).unwrap(), before);
    }

    #[test]
    fn a_damaged_file_is_refused() {
        let directory = scratch("damaged");
        let vault = Vault::open(&directory, KeySource::File(directory.join("key"))).unwrap();
        vault.save(&some_keyring()).unwrap();
        let mut bytes = std::fs::read(directory.join(SECRETS)).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(directory.join(SECRETS), &bytes).unwrap();

        assert!(matches!(vault.load(), Err(VaultError::WrongKey(_))));

        std::fs::write(directory.join(SECRETS), b"{}").unwrap();
        assert!(matches!(vault.load(), Err(VaultError::NotAKeyring(_))));
    }

    #[test]
    fn a_keyring_whose_sealed_key_is_gone_refuses_to_start() {
        let directory = scratch("key-lost");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(SECRETS), b"anything").unwrap();

        let opened = Vault::open(&directory, KeySource::Sealed);

        assert!(matches!(opened, Err(VaultError::KeyLost(_, _))));
        assert_eq!(std::fs::read(directory.join(SECRETS)).unwrap(), b"anything");
    }
}
