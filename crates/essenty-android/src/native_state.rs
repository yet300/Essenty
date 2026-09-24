//! `NativeActivity` saved-state bridge and its versioned byte container.

use std::collections::BTreeMap;

use essenty_state_keeper::StateKeeperError;

const MAGIC: &[u8; 4] = b"ESTS";
const LEGACY_MAGIC: &[u8; 4] = b"EST1";
const VERSION: u8 = 1;
/// Android saved state shares a Binder transaction with other Activity state.
/// Reserve half the platform's 1 MiB transaction budget for host/framework data.
const MAX_ENVELOPE_BYTES: usize = 512 * 1024;

/// Invalid `NativeActivity` saved-state bytes or a failed state provider.
#[derive(Debug)]
pub enum NativeStateError {
    /// The restored byte container is truncated or malformed.
    Malformed,
    /// The state map exceeds the container's `u32` field lengths.
    TooLarge,
    /// The envelope uses a version this implementation does not understand.
    UnsupportedVersion(u8),
    /// The payload checksum does not match the serialized state.
    ChecksumMismatch,
    /// A registered provider failed while saving.
    State(StateKeeperError),
}

impl std::fmt::Display for NativeStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed => f.write_str("malformed NativeActivity saved state"),
            Self::TooLarge => f.write_str("NativeActivity saved state exceeds format limits"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported NativeActivity saved-state version {version}")
            }
            Self::ChecksumMismatch => f.write_str("NativeActivity saved-state checksum mismatch"),
            Self::State(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for NativeStateError {}

impl From<StateKeeperError> for NativeStateError {
    fn from(error: StateKeeperError) -> Self {
        Self::State(error)
    }
}

/// Encodes an ordered map without constraining its per-key payload format.
///
/// # Errors
/// Returns [`NativeStateError::TooLarge`] when the map exceeds format limits.
pub fn encode_native_state(saved: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, NativeStateError> {
    let mut payload = Vec::new();
    let count = u32::try_from(saved.len()).map_err(|_| NativeStateError::TooLarge)?;
    payload.extend_from_slice(&count.to_le_bytes());
    for (key, value) in saved {
        let key_len = u32::try_from(key.len()).map_err(|_| NativeStateError::TooLarge)?;
        let value_len = u32::try_from(value.len()).map_err(|_| NativeStateError::TooLarge)?;
        payload.extend_from_slice(&key_len.to_le_bytes());
        payload.extend_from_slice(&value_len.to_le_bytes());
        payload.extend_from_slice(key.as_bytes());
        payload.extend_from_slice(value);
    }
    let total = 9_usize.checked_add(payload.len()).ok_or(NativeStateError::TooLarge)?;
    if total > MAX_ENVELOPE_BYTES {
        return Err(NativeStateError::TooLarge);
    }
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&crc32(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], NativeStateError> {
    let end = cursor.checked_add(len).ok_or(NativeStateError::Malformed)?;
    let value = bytes.get(*cursor..end).ok_or(NativeStateError::Malformed)?;
    *cursor = end;
    Ok(value)
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, NativeStateError> {
    Ok(u32::from_le_bytes(
        take(bytes, cursor, 4)?.try_into().map_err(|_| NativeStateError::Malformed)?,
    ))
}

/// Decodes the `NativeActivity` container and rejects duplicate or corrupt keys.
///
/// # Errors
/// Returns [`NativeStateError::Malformed`] for invalid bytes.
pub fn decode_native_state(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, NativeStateError> {
    if bytes.is_empty() {
        return Ok(BTreeMap::new());
    }
    if bytes.len() > MAX_ENVELOPE_BYTES || bytes.len() < 8 {
        return Err(NativeStateError::Malformed);
    }
    if &bytes[..4] == LEGACY_MAGIC {
        return decode_records(&bytes[4..]);
    }
    if bytes.len() < 9 || &bytes[..4] != MAGIC {
        return Err(NativeStateError::Malformed);
    }
    if bytes[4] != VERSION {
        return Err(NativeStateError::UnsupportedVersion(bytes[4]));
    }
    let expected =
        u32::from_le_bytes(bytes[5..9].try_into().map_err(|_| NativeStateError::Malformed)?);
    let payload = &bytes[9..];
    if crc32(payload) != expected {
        return Err(NativeStateError::ChecksumMismatch);
    }
    decode_records(payload)
}

fn decode_records(payload: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, NativeStateError> {
    let mut cursor = 0;
    let count = read_u32(payload, &mut cursor)?;
    let mut restored = BTreeMap::new();
    for _ in 0..count {
        let key_len = read_u32(payload, &mut cursor)? as usize;
        let value_len = read_u32(payload, &mut cursor)? as usize;
        let key = std::str::from_utf8(take(payload, &mut cursor, key_len)?)
            .map_err(|_| NativeStateError::Malformed)?
            .to_owned();
        let value = take(payload, &mut cursor, value_len)?.to_vec();
        if restored.insert(key, value).is_some() {
            return Err(NativeStateError::Malformed);
        }
    }
    if cursor != payload.len() {
        return Err(NativeStateError::Malformed);
    }
    Ok(restored)
}

#[cfg(all(target_os = "android", feature = "native-activity"))]
mod activity {
    use std::collections::BTreeMap;

    use android_activity::{StateLoader, StateSaver};
    use essenty_state_keeper::StateKeeper;

    use super::{NativeStateError, decode_native_state, encode_native_state};

    /// Owns the Rust state keeper for a `NativeActivity` host.
    #[derive(Debug)]
    pub struct NativeActivityState {
        keeper: StateKeeper,
    }

    impl NativeActivityState {
        /// Restores bytes supplied by `MainEvent::Resume`'s state loader.
        ///
        /// # Errors
        /// Returns an error if a nonempty saved container is malformed.
        pub fn from_loader(loader: &StateLoader<'_>) -> Result<Self, NativeStateError> {
            Self::from_bytes(loader.load().as_deref().unwrap_or_default())
        }

        /// Restores a versioned container supplied by an Android host.
        ///
        /// # Errors
        /// Returns an error if a nonempty saved container is malformed.
        pub fn from_bytes(bytes: &[u8]) -> Result<Self, NativeStateError> {
            let restored =
                if bytes.is_empty() { BTreeMap::default() } else { decode_native_state(bytes)? };
            Ok(Self { keeper: StateKeeper::with_restored(restored) })
        }

        /// Mutable keeper for provider registration and single-shot consumption.
        #[must_use]
        pub fn keeper_mut(&mut self) -> &mut StateKeeper {
            &mut self.keeper
        }

        /// Captures a versioned byte container during `MainEvent::SaveState`.
        /// Pass the returned bytes to that event's `StateSaver::store`.
        ///
        /// # Errors
        /// Returns an error if a provider fails or size limits are exceeded.
        pub fn save_bytes(&self) -> Result<Vec<u8>, NativeStateError> {
            encode_native_state(&self.keeper.save()?)
        }

        /// Stores a precomputed snapshot during `MainEvent::SaveState`.
        /// Keeping the bytes owned by the event caller satisfies the saver
        /// lifetime; `android-activity` copies them into native storage.
        pub fn store<'a>(saver: &StateSaver<'a>, bytes: &'a [u8]) {
            saver.store(bytes);
        }
    }
}

#[cfg(all(target_os = "android", feature = "native-activity"))]
pub use activity::NativeActivityState;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_state_round_trip_preserves_opaque_values() {
        let saved = BTreeMap::from([("a".to_owned(), vec![0, 255]), ("b".to_owned(), vec![])]);
        assert_eq!(decode_native_state(&encode_native_state(&saved).unwrap()).unwrap(), saved);
    }

    #[test]
    fn native_state_rejects_truncation_and_duplicate_keys() {
        let saved = BTreeMap::from([("a".to_owned(), vec![1])]);
        let encoded = encode_native_state(&saved).unwrap();
        for length in 1..encoded.len() {
            assert!(decode_native_state(&encoded[..length]).is_err());
        }
        let mut duplicate = encoded.clone();
        duplicate[4..8].copy_from_slice(&2_u32.to_le_bytes());
        duplicate.extend_from_slice(&encoded[8..]);
        assert!(decode_native_state(&duplicate).is_err());
    }

    #[test]
    fn native_state_distinguishes_empty_unsupported_and_corrupt_envelopes() {
        assert!(decode_native_state(&[]).unwrap().is_empty());

        let mut unsupported = encode_native_state(&BTreeMap::new()).unwrap();
        unsupported[4] = 9;
        assert!(matches!(
            decode_native_state(&unsupported),
            Err(NativeStateError::UnsupportedVersion(9))
        ));

        let mut corrupt =
            encode_native_state(&BTreeMap::from([("key".into(), vec![1, 2, 3])])).unwrap();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 0xff;
        assert!(matches!(decode_native_state(&corrupt), Err(NativeStateError::ChecksumMismatch)));
    }

    #[test]
    fn native_state_rejects_payloads_over_android_transaction_limit() {
        let bytes = vec![0; MAX_ENVELOPE_BYTES];
        let state = BTreeMap::from([("large".to_owned(), bytes)]);
        assert!(matches!(encode_native_state(&state), Err(NativeStateError::TooLarge)));
    }

    #[test]
    fn state_not_consumed_before_another_save_survives() {
        let restored = BTreeMap::from([("pending".to_owned(), vec![3, 1, 4])]);
        let keeper = essenty_state_keeper::StateKeeper::with_restored(restored.clone());
        let native = encode_native_state(&keeper.save().unwrap()).unwrap();
        assert_eq!(decode_native_state(&native).unwrap(), restored);
    }

    #[test]
    fn state_envelope_reads_legacy_est1_snapshots() {
        // EST1 encoded count + key/value lengths with no checksum.
        let mut legacy = b"EST1".to_vec();
        legacy.extend_from_slice(&1_u32.to_le_bytes());
        legacy.extend_from_slice(&1_u32.to_le_bytes());
        legacy.extend_from_slice(&2_u32.to_le_bytes());
        legacy.extend_from_slice(b"k");
        legacy.extend_from_slice(&[7, 8]);
        assert_eq!(
            decode_native_state(&legacy).unwrap(),
            BTreeMap::from([("k".into(), vec![7, 8])])
        );
    }
}
