use aes::{
    cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit},
    Aes256,
};
use num_bigint::{BigUint, RandBigInt};
use rand::thread_rng;
use sha2::{Digest, Sha256};
//Currently unused, this pile of jank should interoperate with Photon message encryption, it's just not implemented since true fake client is
// blocked on EAC reversing and we obviously can't decrypt messages from the real client without its secret
pub struct Encryption {
    pub prime: BigUint,
    pub secret: BigUint,
    pub public_key: BigUint,
    pub cipher: Option<(cbc::Encryptor<Aes256>, cbc::Decryptor<Aes256>)>,
}
impl Encryption {
    pub fn new() -> Self {
        let prime = BigUint::from_bytes_be(&[
            255, 255, 255, 255, 255, 255, 255, 255, 201, 15, 218, 162, 33, 104, 194, 52, 196, 198,
            98, 139, 128, 220, 28, 209, 41, 2, 78, 8, 138, 103, 204, 116, 2, 11, 190, 166, 59, 19,
            155, 34, 81, 74, 8, 121, 142, 52, 4, 221, 239, 149, 25, 179, 205, 58, 67, 27, 48, 43,
            10, 109, 242, 95, 20, 55, 79, 225, 53, 109, 109, 81, 194, 69, 228, 133, 181, 118, 98,
            94, 126, 198, 244, 76, 66, 233, 166, 58, 54, 32, 255, 255, 255, 255, 255, 255, 255,
            255,
        ]);
        let mut r = thread_rng();
        let secret = r.gen_biguint(160 * 8);
        let public_key = BigUint::from(22u32).modpow(&secret, &prime);
        Self {
            prime,
            secret,
            public_key,
            cipher: None,
        }
    }
    pub fn make_shared_key(&mut self, other_public_key_buf: &[u8]) {
        let b = BigUint::from_bytes_be(other_public_key_buf);
        let shared_key = b.modpow(&self.secret, &self.prime);
        let bytes = shared_key.to_bytes_be();
        let mut s = Sha256::new();
        s.update(&bytes);
        let key_arr = s.finalize();
        self.cipher = Some((
            cbc::Encryptor::new(&key_arr, &[0u8; 16].into()),
            cbc::Decryptor::new(&key_arr, &[0u8; 16].into()),
        ));
    }
    pub fn decrypt<'a>(&self, buf: &'a [u8]) -> Option<Vec<u8>> {
        let dec = &self.cipher.as_ref()?.1;
        let mut v = Vec::from(buf);
        let val = dec
            .clone()
            .decrypt_padded_mut::<block_padding::Pkcs7>(&mut v)
            .map_err(|_| println!("ERROR: Crypto Unpad Error"))
            .ok()?;
        Some(Vec::from(val))
    }
    pub fn encrypt<'a>(&self, buf: &'a [u8]) -> Option<Vec<u8>> {
        let dec = &self.cipher.as_ref()?.0;
        let mut v = Vec::from(buf);
        let l = v.len();
        v.resize(l + 16, 0);
        let s = dec
            .clone()
            .encrypt_padded_mut::<block_padding::Pkcs7>(&mut v, l)
            .map_err(|_| println!("ERROR: Crypto Pad Error"))
            .ok()?;
        Some(Vec::from(s))
    }
}

#[cfg(test)]
mod tests {
    use super::Encryption;

    #[test]
    fn test_enc_dec() {
        let mut c1 = Encryption::new();
        let mut c2 = Encryption::new();
        c1.make_shared_key(&c2.public_key.to_bytes_be());
        c2.make_shared_key(&c1.public_key.to_bytes_be());
        assert_eq!(
            &[0x1u8, 0x1, 0x1],
            &c2.decrypt(&c1.encrypt(&[0x1, 0x1, 0x1]).unwrap()).unwrap() as &[u8]
        );
    }
}
