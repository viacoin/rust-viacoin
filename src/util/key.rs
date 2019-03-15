// Rust Bitcoin Library
// Written in 2014 by
//     Andrew Poelstra <apoelstra@wpsoftware.net>
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! Bitcoin Keys
//!
//! Keys used in Bitcoin that can be roundtrip (de)serialized.
//!

use std::fmt::{self, Write};
use std::{io, ops};
use std::str::FromStr;
use secp256k1::{self, Secp256k1};
use consensus::encode;
use network::constants::Network;
use util::base58;

/// A Bitcoin ECDSA public key
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicKey {
    /// Whether this public key should be serialized as compressed
    pub compressed: bool,
    /// The actual ECDSA key
    pub key: secp256k1::PublicKey,
}

impl PublicKey {
    /// Write the public key into a writer
    pub fn write_into<W: io::Write>(&self, mut writer: W) {
        let write_res: io::Result<()> = if self.compressed {
            writer.write_all(&self.key.serialize())
        } else {
            writer.write_all(&self.key.serialize_uncompressed())
        };
        debug_assert!(write_res.is_ok());
    }

    /// Serialize the public key to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.write_into(&mut buf);
        buf
    }

    /// Deserialize a public key from a slice
    pub fn from_slice(data: &[u8]) -> Result<PublicKey, encode::Error> {
        let compressed: bool = match data.len() {
            33 => true,
            65 => false,
            len =>  { return Err(base58::Error::InvalidLength(len).into()); },
        };

        Ok(PublicKey {
            compressed: compressed,
            key: secp256k1::PublicKey::from_slice(data)?,
        })
    }

    /// Computes the public key as supposed to be used with this secret
    pub fn from_private_key<C: secp256k1::Signing>(secp: &Secp256k1<C>, sk: &PrivateKey) -> PublicKey {
        sk.public_key(secp)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.compressed {
            for ch in &self.key.serialize()[..] {
                write!(f, "{:02x}", ch)?;
            }
        } else {
            for ch in &self.key.serialize_uncompressed()[..] {
                write!(f, "{:02x}", ch)?;
            }
        }
        Ok(())
    }
}

impl FromStr for PublicKey {
    type Err = encode::Error;
    fn from_str(s: &str) -> Result<PublicKey, encode::Error> {
        let key = secp256k1::PublicKey::from_str(s)?;
        Ok(PublicKey {
            key: key,
            compressed: s.len() == 66
        })
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
/// A Bitcoin ECDSA private key
pub struct PrivateKey {
    /// Whether this private key should be serialized as compressed
    pub compressed: bool,
    /// The network on which this key should be used
    pub network: Network,
    /// The actual ECDSA key
    pub key: secp256k1::SecretKey,
}

impl PrivateKey {
    /// Creates a public key from this private key
    pub fn public_key<C: secp256k1::Signing>(&self, secp: &Secp256k1<C>) -> PublicKey {
        PublicKey {
            compressed: self.compressed,
            key: secp256k1::PublicKey::from_secret_key(secp, &self.key)
        }
    }

    /// Serialize the private key to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.key[..].to_vec()
    }

    /// Format the private key to WIF format.
    pub fn fmt_wif(&self, fmt: &mut fmt::Write) -> fmt::Result {
        let mut ret = [0; 34];
        ret[0] = match self.network {
            Network::Bitcoin => 199,
            Network::Testnet | Network::Regtest => 239,
        };
        ret[1..33].copy_from_slice(&self.key[..]);
        let privkey = if self.compressed {
            ret[33] = 1;
            base58::check_encode_slice(&ret[..])
        } else {
            base58::check_encode_slice(&ret[..33])
        };
        fmt.write_str(&privkey)
    }

    /// Get WIF encoding of this private key.
    pub fn to_wif(&self) -> String {
        let mut buf = String::new();
        buf.write_fmt(format_args!("{}", self)).unwrap();
        buf.shrink_to_fit();
        buf
    }

    /// Parse WIF encoded private key.
    pub fn from_wif(wif: &str) -> Result<PrivateKey, encode::Error> {
        let data = base58::from_check(wif)?;

        let compressed = match data.len() {
            33 => false,
            34 => true,
            _ => { return Err(encode::Error::Base58(base58::Error::InvalidLength(data.len()))); }
        };

        let network = match data[0] {
            199 => Network::Bitcoin,
            239 => Network::Testnet,
            x   => { return Err(encode::Error::Base58(base58::Error::InvalidVersion(vec![x]))); }
        };

        Ok(PrivateKey {
            compressed: compressed,
            network: network,
            key: secp256k1::SecretKey::from_slice(&data[1..33])?,
        })
    }
}

impl fmt::Display for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.fmt_wif(f)
    }
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[private key data]")
    }
}

impl FromStr for PrivateKey {
    type Err = encode::Error;
    fn from_str(s: &str) -> Result<PrivateKey, encode::Error> {
        PrivateKey::from_wif(s)
    }
}

impl ops::Index<ops::RangeFull> for PrivateKey {
    type Output = [u8];
    fn index(&self, _: ops::RangeFull) -> &[u8] {
        &self.key[..]
    }
}

#[cfg(test)]
mod tests {
    use super::{PrivateKey, PublicKey};
    use secp256k1::Secp256k1;
    use std::str::FromStr;
    use network::constants::Network::Testnet;
    use network::constants::Network::Bitcoin;
    use util::address::Address;

    #[test]
    fn test_key_derivation() {
        // testnet compressed
        let sk = PrivateKey::from_wif("cVt4o7BGAig1UXywgGSmARhxMdzP5qvQsxKkSsc1XEkw3tDTQFpy").unwrap();
        assert_eq!(sk.network, Testnet);
        assert_eq!(sk.compressed, true);
        assert_eq!(&sk.to_wif(), "cVt4o7BGAig1UXywgGSmARhxMdzP5qvQsxKkSsc1XEkw3tDTQFpy");

        let secp = Secp256k1::new();
        let pk = Address::p2pkh(&sk.public_key(&secp), sk.network);
        assert_eq!(&pk.to_string(), "mqwpxxvfv3QbM8PU8uBx2jaNt9btQqvQNx");

        // test string conversion
        assert_eq!(&sk.to_string(), "cVt4o7BGAig1UXywgGSmARhxMdzP5qvQsxKkSsc1XEkw3tDTQFpy");
        let sk_str =
            PrivateKey::from_str("cVt4o7BGAig1UXywgGSmARhxMdzP5qvQsxKkSsc1XEkw3tDTQFpy").unwrap();
        assert_eq!(&sk.to_wif(), &sk_str.to_wif());

        // mainnet uncompressed
        let sk = PrivateKey::from_wif("7gLEdvAfpYMHai6kzaNvurMjhqizH9Fyz3LijTaCZ2Lxyxme8Yo").unwrap();
        assert_eq!(sk.network, Bitcoin);
        assert_eq!(sk.compressed, false);
        assert_eq!(&sk.to_wif(), "7gLEdvAfpYMHai6kzaNvurMjhqizH9Fyz3LijTaCZ2Lxyxme8Yo");

        let secp = Secp256k1::new();
        let mut pk = sk.public_key(&secp);
        assert_eq!(pk.compressed, false);
        assert_eq!(&pk.to_string(), "043b8f2b8f1e4cffe479c512a082306306e39b28961c3e8e6f91ff31cfa7d46faad951cc2e10702857d7c9389ef7ef82886b69430358e72992fbbd0bcde709c3bc");
        assert_eq!(pk, PublicKey::from_str("043b8f2b8f1e4cffe479c512a082306306e39b28961c3e8e6f91ff31cfa7d46faad951cc2e10702857d7c9389ef7ef82886b69430358e72992fbbd0bcde709c3bc").unwrap());
        let addr = Address::p2pkh(&pk, sk.network);
        assert_eq!(&addr.to_string(), "VbnnCpepvFv8puAAwRYJeKTXQeYieMwKcn");
        pk.compressed = true;
        assert_eq!(&pk.to_string(), "023b8f2b8f1e4cffe479c512a082306306e39b28961c3e8e6f91ff31cfa7d46faa");
        assert_eq!(pk, PublicKey::from_str("023b8f2b8f1e4cffe479c512a082306306e39b28961c3e8e6f91ff31cfa7d46faa").unwrap());
    }
}
