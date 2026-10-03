//! §3 and §5 — who wrote an object, proven: detached signatures, keys
//! registered by objects, and the reading of both.
//!
//! See ../../SPEC.md §3 (the objects) and §5 (the policy that asks for
//! them). An object's `actor` is a field its writer chose. A SIGNATURE is an
//! object of its own, `Signed(object, key, signature)`, naming what it signs
//! by its content name, so no object's bytes or name change when it is signed
//! and an object can gain signatures at any time after it is written. A key
//! belongs to an actor because an object says so, `KeyAdded(genesis, actor,
//! key)`, and stops proving what it did not already prove because one says
//! so, `KeyRevoked(genesis, deps, actor, key)`. So which key speaks for whom
//! is a READING of the history, like every register, and [`Proof`] is that
//! reading: a function of the object set, whatever order it arrived in.

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

use crate::event::{field, hashes, hashes_value, lower_hex, sorted_distinct, Actor, Hash};
use crate::literal::{Call, ProdromeError, Value};
use crate::payload::string_field;

crate::newtype_str! {
    /// An Ed25519 public key: its 32 bytes as 64 lowercase hex.
    PublicKey
}

crate::newtype_str! {
    /// An Ed25519 signature: its 64 bytes as 128 lowercase hex.
    Signature
}

impl PublicKey {
    /// 64 lowercase hex that ARE a key: the encoding of a point on the curve.
    ///
    /// # Errors
    ///
    /// Anything else, refused as a field of an object is.
    pub fn new(text: impl Into<String>) -> Result<PublicKey, ProdromeError> {
        let text = text.into();
        bytes::<32>(&text)
            .filter(|bytes| VerifyingKey::from_bytes(bytes).is_ok())
            .map(|_| PublicKey(text.clone()))
            .ok_or_else(|| {
                ProdromeError::invalid(format!(
                    "a public key must be 64 lowercase hex encoding an Ed25519 point, got {text:?}"
                ))
            })
    }

    fn verifying(&self) -> VerifyingKey {
        let bytes = bytes::<32>(&self.0).expect("checked by the constructor");
        VerifyingKey::from_bytes(&bytes).expect("checked by the constructor")
    }
}

impl Signature {
    /// 128 lowercase hex. Whether it verifies is [`Signed::verifies`]'s
    /// question, asked of a signature and what it signs together.
    ///
    /// # Errors
    ///
    /// Anything else.
    pub fn new(text: impl Into<String>) -> Result<Signature, ProdromeError> {
        let text = text.into();
        if lower_hex(&text, 128) {
            Ok(Signature(text))
        } else {
            Err(ProdromeError::invalid(format!(
                "a signature must be 128 lowercase hex, got {text:?}"
            )))
        }
    }
}

/// `text` as `N` bytes, where it is `2N` lowercase hex.
fn bytes<const N: usize>(text: &str) -> Option<[u8; N]> {
    if !lower_hex(text, 2 * N) {
        return None;
    }
    let mut out = [0; N];
    for (byte, pair) in out.iter_mut().zip(text.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// What a signature of `object` signs: the bytes `prodrome object `, then
/// the object's name. The prefix keeps a key that signs objects from
/// signing anything else by accident.
fn message(object: &Hash) -> Vec<u8> {
    let mut message = b"prodrome object ".to_vec();
    message.extend_from_slice(object.as_str().as_bytes());
    message
}

/// `Signed(object, key, signature)`: `key`'s signature of the object named
/// `object`. Its one parent is that object, so it joins a history only with
/// what it signs, and its prodrome is that object's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub object: Hash,
    pub key: PublicKey,
    pub signature: Signature,
}

impl Signed {
    /// Does the signature verify, under its key, over its object's name?
    /// Strictly: a signature with a small-order key or a non-canonical
    /// scalar is refused, so one key and one message admit no second
    /// encoding a forger could compute from the first.
    #[must_use]
    pub fn verifies(&self) -> bool {
        let signature = bytes::<64>(self.signature.as_str())
            .map(|bytes| ed25519_dalek::Signature::from_bytes(&bytes));
        signature.is_some_and(|signature| {
            self.key
                .verifying()
                .verify_strict(&message(&self.object), &signature)
                .is_ok()
        })
    }

    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::call(
            "Signed",
            vec![
                field("object", Value::str(self.object.as_str())),
                field("key", Value::str(self.key.as_str())),
                field("signature", Value::str(self.signature.as_str())),
            ],
        )
    }

    /// # Errors
    ///
    /// A field missing, or refused by its type.
    pub fn from_call(call: &Call) -> Result<Signed, ProdromeError> {
        Ok(Signed {
            object: Hash::new(string_field(call, "object")?)?,
            key: PublicKey::new(string_field(call, "key")?)?,
            signature: Signature::new(string_field(call, "signature")?)?,
        })
    }
}

/// `KeyAdded(genesis, actor, key)`: in the prodrome `genesis`, `key`
/// speaks for `actor`. No parents but its genesis: adding a key twice is
/// one object, and a key's signatures need not come after it (a device
/// signs offline, and is registered when it is next seen).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyAdded {
    pub genesis: Hash,
    pub actor: Actor,
    pub key: PublicKey,
}

impl KeyAdded {
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::call(
            "KeyAdded",
            vec![
                field("genesis", Value::str(self.genesis.as_str())),
                field("actor", Value::str(self.actor.as_str())),
                field("key", Value::str(self.key.as_str())),
            ],
        )
    }

    /// # Errors
    ///
    /// A field missing, or refused by its type.
    pub fn from_call(call: &Call) -> Result<KeyAdded, ProdromeError> {
        Ok(KeyAdded {
            genesis: Hash::new(string_field(call, "genesis")?)?,
            actor: Actor::new(string_field(call, "actor")?)?,
            key: PublicKey::new(string_field(call, "key")?)?,
        })
    }
}

/// `KeyRevoked(genesis, deps, actor, key)`: `key` no longer speaks for
/// `actor`, except in the signatures this object rests on. `deps` are what
/// its writer stands behind, sorted and distinct, as a change's are what
/// its writer saw: a signature by the key beneath them still proves, and
/// one that is not (made after, or concurrently, or never seen) does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRevoked {
    pub genesis: Hash,
    pub deps: Vec<Hash>,
    pub actor: Actor,
    pub key: PublicKey,
}

/// A revocation, its deps sorted and distinct.
///
/// # Errors
///
/// A dep named twice.
pub fn mk_key_revoked(
    genesis: Hash,
    deps: Vec<Hash>,
    actor: Actor,
    key: PublicKey,
) -> Result<KeyRevoked, ProdromeError> {
    Ok(KeyRevoked {
        genesis,
        deps: sorted_distinct("KeyRevoked.deps", "dep", deps)?,
        actor,
        key,
    })
}

impl KeyRevoked {
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::call(
            "KeyRevoked",
            vec![
                field("genesis", Value::str(self.genesis.as_str())),
                field("deps", hashes_value(&self.deps)),
                field("actor", Value::str(self.actor.as_str())),
                field("key", Value::str(self.key.as_str())),
            ],
        )
    }

    /// # Errors
    ///
    /// A field missing, or refused by its type.
    pub fn from_call(call: &Call) -> Result<KeyRevoked, ProdromeError> {
        mk_key_revoked(
            Hash::new(string_field(call, "genesis")?)?,
            hashes(call, "deps")?,
            Actor::new(string_field(call, "actor")?)?,
            PublicKey::new(string_field(call, "key")?)?,
        )
    }
}

/// A device's signing key: the 32-byte seed the host keeps wherever it
/// decides (a phone in its own storage, a box in a file), never an object.
/// Signing is deterministic, so one key signing one object writes one
/// `Signed`, byte for byte, wherever and however often it signs.
pub struct Secret(SigningKey);

impl Secret {
    /// The key whose seed is `seed`, 64 lowercase hex. The host draws it
    /// from its own source of randomness; this crate has none and no clock.
    ///
    /// # Errors
    ///
    /// A seed that is not 64 lowercase hex.
    pub fn new(seed: &str) -> Result<Secret, ProdromeError> {
        bytes::<32>(seed)
            .map(|seed| Secret(SigningKey::from_bytes(&seed)))
            .ok_or_else(|| ProdromeError::invalid("a secret key must be 64 lowercase hex"))
    }

    #[must_use]
    pub fn public(&self) -> PublicKey {
        PublicKey(hex(self.0.verifying_key().as_bytes()))
    }

    /// This key's signature of the object named `object`.
    #[must_use]
    pub fn sign(&self, object: &Hash) -> Signed {
        Signed {
            object: object.clone(),
            key: self.public(),
            signature: Signature(hex(&self.0.sign(&message(object)).to_bytes())),
        }
    }
}

/// No secret is printed, even to a log.
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Secret").field(&self.public()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{parse_envelope, seal_hash, Envelope, TodoEvent};
    use crate::literal::print_literal;
    use crate::reference::Todo;

    type Object = Envelope<TodoEvent<Todo>>;

    /// RFC 8032 §7.1, TEST 1: the seed, and the key it derives.
    const SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    const PUBLIC: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

    fn name(text: &str) -> Hash {
        Hash::of_bytes(text.as_bytes())
    }

    fn round_trips(object: &Object) {
        let print = print_literal(&object.to_value());
        let back: Object = parse_envelope(&Default::default(), &print).expect("parses");
        assert_eq!(&back, object);
        assert_eq!(seal_hash(&back), Hash::of_bytes(print.as_bytes()));
    }

    #[test]
    fn a_seed_derives_rfc_8032s_key() {
        assert_eq!(Secret::new(SEED).expect("a seed").public().as_str(), PUBLIC);
    }

    #[test]
    fn each_object_prints_and_parses_back() {
        let secret = Secret::new(SEED).expect("a seed");
        let key = secret.public();
        let actor = Actor::new("bassel").expect("an actor");
        round_trips(&Envelope::Signed(secret.sign(&name("a change"))));
        round_trips(&Envelope::KeyAdded(KeyAdded {
            genesis: name("a genesis"),
            actor: actor.clone(),
            key: key.clone(),
        }));
        round_trips(&Envelope::KeyRevoked(
            mk_key_revoked(name("a genesis"), vec![name("b"), name("a")], actor, key)
                .expect("distinct deps"),
        ));
    }

    #[test]
    fn a_signature_verifies_over_its_object_and_nothing_else() {
        let secret = Secret::new(SEED).expect("a seed");
        let signed = secret.sign(&name("a change"));
        assert!(signed.verifies());
        assert_eq!(
            signed,
            secret.sign(&name("a change")),
            "signing is deterministic"
        );
        let moved = Signed {
            object: name("another change"),
            ..signed.clone()
        };
        assert!(
            !moved.verifies(),
            "a signature does not move to another object"
        );
        let other = Secret::new(&"1".repeat(64)).expect("a seed").public();
        assert!(!Signed {
            key: other,
            ..signed.clone()
        }
        .verifies());
        let mut flipped = signed.signature.as_str().to_owned();
        flipped.replace_range(0..1, if flipped.starts_with('0') { "1" } else { "0" });
        let flipped = Signed {
            signature: Signature::new(flipped).expect("hex"),
            ..signed
        };
        assert!(!flipped.verifies());
    }

    #[test]
    fn a_key_is_a_point_and_a_signature_is_hex() {
        assert!(PublicKey::new(PUBLIC).is_ok());
        assert!(PublicKey::new(PUBLIC.to_uppercase()).is_err());
        assert!(PublicKey::new(&PUBLIC[2..]).is_err());
        // The y-coordinate 2 is on no point of the curve.
        let off = format!("02{}", "0".repeat(62));
        assert!(PublicKey::new(off).is_err());
        assert!(Signature::new("0".repeat(128)).is_ok());
        assert!(Signature::new("0".repeat(127)).is_err());
        assert!(Secret::new("not hex").is_err());
    }

    #[test]
    fn a_revocation_names_each_dep_once() {
        let key = PublicKey::new(PUBLIC).expect("a key");
        let actor = Actor::new("bassel").expect("an actor");
        assert!(mk_key_revoked(name("g"), vec![name("a"), name("a")], actor, key).is_err());
    }

    #[test]
    fn a_secret_is_never_printed() {
        let printed = format!("{:?}", Secret::new(SEED).expect("a seed"));
        assert!(!printed.contains(SEED) && printed.contains(PUBLIC));
    }
}
