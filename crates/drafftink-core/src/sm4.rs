//! # SM4 国密对称加密（GB/T 32907-2016）
//!
//! 纯 Rust 实现的 SM4 分组密码，**无任何 C 依赖**，用于移动办公场景下
//! 敏感数据（用印事由、审批意见等）的校内传输加密，满足《教育数据安全管理规范》
//! 对国密算法的合规要求。
//!
//! - 分组长度 128 位（16 字节），密钥长度 128 位（16 字节）。
//! - 提供 **ECB**（仅用于合规测试向量与定长场景）与 **CBC**（生产推荐）两种模式，
//!   均使用 **PKCS#7** 填充。CBC 消除 ECB「相同明文→相同密文」的语义安全缺陷。
//! - S 盒为 GB/T 32907-2016 表 1 的权威硬编码常量 [`SM4_SBOX`]，
//!   并以 GM/T 0002-2012 附录 A 标准测试向量（KAT）校验实现正确性。

/// SM4 S-box（GB/T 32907-2016 表 1 / GM/T 0002-2012 附录）
///
/// 权威硬编码常量，行优先排列，索引 `0x00` → `0xFF`。
/// 来源：密码行业标准化技术委员会 GM/T 0002-2012 附录 A。
pub const SM4_SBOX: [u8; 256] = [
    // 0x00
    0xD6, 0x90, 0xE9, 0xFE, 0xCC, 0xE1, 0x3D, 0xB7, 0x16, 0xB6, 0x14, 0xC2, 0x28, 0xFB, 0x2C, 0x05,
    // 0x10
    0x2B, 0x67, 0x9A, 0x76, 0x2A, 0xBE, 0x04, 0xC3, 0xAA, 0x44, 0x13, 0x26, 0x49, 0x86, 0x06, 0x99,
    // 0x20
    0x9C, 0x42, 0x50, 0xF4, 0x91, 0xEF, 0x98, 0x7A, 0x33, 0x54, 0x0B, 0x43, 0xED, 0xCF, 0xAC, 0x62,
    // 0x30
    0xE4, 0xB3, 0x1C, 0xA9, 0xC9, 0x08, 0xE8, 0x95, 0x80, 0xDF, 0x94, 0xFA, 0x75, 0x8F, 0x3F, 0xA6,
    // 0x40
    0x47, 0x07, 0xA7, 0xFC, 0xF3, 0x73, 0x17, 0xBA, 0x83, 0x59, 0x3C, 0x19, 0xE6, 0x85, 0x4F, 0xA8,
    // 0x50
    0x68, 0x6B, 0x81, 0xB2, 0x71, 0x64, 0xDA, 0x8B, 0xF8, 0xEB, 0x0F, 0x4B, 0x70, 0x56, 0x9D, 0x35,
    // 0x60
    0x1E, 0x24, 0x0E, 0x5E, 0x63, 0x58, 0xD1, 0xA2, 0x25, 0x22, 0x7C, 0x3B, 0x01, 0x21, 0x78, 0x87,
    // 0x70
    0xD4, 0x00, 0x46, 0x57, 0x9F, 0xD3, 0x27, 0x52, 0x4C, 0x36, 0x02, 0xE7, 0xA0, 0xC4, 0xC8, 0x9E,
    // 0x80
    0xEA, 0xBF, 0x8A, 0xD2, 0x40, 0xC7, 0x38, 0xB5, 0xA3, 0xF7, 0xF2, 0xCE, 0xF9, 0x61, 0x15, 0xA1,
    // 0x90
    0xE0, 0xAE, 0x5D, 0xA4, 0x9B, 0x34, 0x1A, 0x55, 0xAD, 0x93, 0x32, 0x30, 0xF5, 0x8C, 0xB1, 0xE3,
    // 0xA0
    0x1D, 0xF6, 0xE2, 0x2E, 0x82, 0x66, 0xCA, 0x60, 0xC0, 0x29, 0x23, 0xAB, 0x0D, 0x53, 0x4E, 0x6F,
    // 0xB0
    0xD5, 0xDB, 0x37, 0x45, 0xDE, 0xFD, 0x8E, 0x2F, 0x03, 0xFF, 0x6A, 0x72, 0x6D, 0x6C, 0x5B, 0x51,
    // 0xC0
    0x8D, 0x1B, 0xAF, 0x92, 0xBB, 0xDD, 0xBC, 0x7F, 0x11, 0xD9, 0x5C, 0x41, 0x1F, 0x10, 0x5A, 0xD8,
    // 0xD0
    0x0A, 0xC1, 0x31, 0x88, 0xA5, 0xCD, 0x7B, 0xBD, 0x2D, 0x74, 0xD0, 0x12, 0xB8, 0xE5, 0xB4, 0xB0,
    // 0xE0
    0x89, 0x69, 0x97, 0x4A, 0x0C, 0x96, 0x77, 0x7E, 0x65, 0xB9, 0xF1, 0x09, 0xC5, 0x6E, 0xC6, 0x84,
    // 0xF0
    0x18, 0xF0, 0x7D, 0xEC, 0x3A, 0xDC, 0x4D, 0x20, 0x79, 0xEE, 0x5F, 0x3E, 0xD7, 0xCB, 0x39, 0x48,
];

/// 取 SM4 S-box 映射值。
#[inline(always)]
pub fn sm4_sbox(byte: u8) -> u8 {
    SM4_SBOX[byte as usize]
}

/// FK 系统参数（GB/T 32907）。
const FK: [u32; 4] = [0xA3B1_BAC6, 0x56AA_3350, 0x677D_9197, 0xB270_22DC];

/// 生成第 `i` 个 CK 固定参数。
///
/// SM4 固定常量：第 `i` 个 CK 字由 4 个字节 `ck[j] = (4i + j)·7 mod 256` 拼接而成。
#[inline]
const fn ck(i: usize) -> u32 {
    let base = (i * 4) as u8;
    let b0 = (base.wrapping_mul(7)) as u32;
    let b1 = (base.wrapping_add(1).wrapping_mul(7)) as u32;
    let b2 = (base.wrapping_add(2).wrapping_mul(7)) as u32;
    let b3 = (base.wrapping_add(3).wrapping_mul(7)) as u32;
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

/// 对 32 位字应用 S 盒（逐字节）。
#[inline]
fn tau(a: u32) -> u32 {
    let mut r = 0u32;
    let mut i = 0;
    while i < 4 {
        let byte = ((a >> (8 * i)) & 0xff) as usize;
        r |= (SM4_SBOX[byte] as u32) << (8 * i);
        i += 1;
    }
    r
}

/// 线性变换 L：用于轮函数 T。
#[inline]
fn l_transform(b: u32) -> u32 {
    b ^ b.rotate_left(2) ^ b.rotate_left(10) ^ b.rotate_left(18) ^ b.rotate_left(24)
}

/// 线性变换 L'：用于密钥扩展 T'。
#[inline]
fn l_prime(b: u32) -> u32 {
    b ^ b.rotate_left(13) ^ b.rotate_left(23)
}

/// 轮函数 T = L(τ(·))。
#[inline]
fn t_transform(b: u32) -> u32 {
    l_transform(tau(b))
}

/// 密钥扩展中的 T' = L'(τ(·))。
#[inline]
fn t_prime(b: u32) -> u32 {
    l_prime(tau(b))
}

/// 由 16 字节密钥派生 32 个轮密钥。
fn key_schedule(key: &[u8; 16]) -> [u32; 32] {
    let mk = [
        u32::from_be_bytes([key[0], key[1], key[2], key[3]]),
        u32::from_be_bytes([key[4], key[5], key[6], key[7]]),
        u32::from_be_bytes([key[8], key[9], key[10], key[11]]),
        u32::from_be_bytes([key[12], key[13], key[14], key[15]]),
    ];
    let mut k = [mk[0] ^ FK[0], mk[1] ^ FK[1], mk[2] ^ FK[2], mk[3] ^ FK[3]];
    let mut rk = [0u32; 32];
    let mut i = 0;
    while i < 32 {
        let val = k[0] ^ t_prime(k[1] ^ k[2] ^ k[3] ^ ck(i));
        rk[i] = val;
        k = [k[1], k[2], k[3], val];
        i += 1;
    }
    rk
}

/// 对单个 16 字节分组加/解密（由 `rk` 顺序决定方向）。
fn crypt_block(block: &[u8; 16], rk: &[u32; 32]) -> [u8; 16] {
    let mut x = [
        u32::from_be_bytes([block[0], block[1], block[2], block[3]]),
        u32::from_be_bytes([block[4], block[5], block[6], block[7]]),
        u32::from_be_bytes([block[8], block[9], block[10], block[11]]),
        u32::from_be_bytes([block[12], block[13], block[14], block[15]]),
    ];
    let mut i = 0;
    while i < 32 {
        let x4 = x[0] ^ t_transform(x[1] ^ x[2] ^ x[3] ^ rk[i]);
        x = [x[1], x[2], x[3], x4];
        i += 1;
    }
    let mut out = [0u8; 16];
    // SM4 标准：最后 4 个字需按反序输出（X35 X34 X33 X32）。
    for (j, word) in x.iter().rev().enumerate() {
        let bytes = word.to_be_bytes();
        out[j * 4..j * 4 + 4].copy_from_slice(&bytes);
    }
    out
}

/// PKCS#7 填充到 16 字节整数倍。
fn pad_pkcs7(data: &[u8]) -> Vec<u8> {
    let rem = data.len() % 16;
    let pad = if rem == 0 { 16 } else { 16 - rem };
    let mut out = Vec::with_capacity(data.len() + pad);
    out.extend_from_slice(data);
    out.extend(std::iter::repeat_n(pad as u8, pad));
    out
}

/// 去除 PKCS#7 填充（非法填充返回错误）。
fn unpad_pkcs7(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 16 || data.len() % 16 != 0 {
        return Err("密文长度非法（非 16 字节整数倍）".to_string());
    }
    let pad = *data.last().unwrap() as usize;
    if pad == 0 || pad > 16 {
        return Err("PKCS#7 填充字节非法".to_string());
    }
    if data[data.len() - pad..].iter().any(|b| *b != pad as u8) {
        return Err("PKCS#7 填充内容不一致".to_string());
    }
    Ok(data[..data.len() - pad].to_vec())
}

/// SM4 加密器（ECB + PKCS#7）。
pub struct Sm4 {
    rk: [u32; 32],
}

impl Sm4 {
    /// 由 16 字节密钥构造。
    pub fn new(key: &[u8; 16]) -> Self {
        Self {
            rk: key_schedule(key),
        }
    }

    /// 由任意长度密钥材料派生 16 字节密钥（取 SHA-256 前 16 字节）。
    pub fn from_material(material: &[u8]) -> Self {
        let hash = crate::utils::sha256(material);
        let mut key = [0u8; 16];
        key.copy_from_slice(&hash[..16]);
        Self::new(&key)
    }

    /// 加密单个 16 字节分组（无填充），用于合规测试向量与定长场景。
    pub fn encrypt_block(&self, block: &[u8; 16]) -> [u8; 16] {
        crypt_block(block, &self.rk)
    }

    /// 解密单个 16 字节分组（无填充）。
    pub fn decrypt_block(&self, block: &[u8; 16]) -> [u8; 16] {
        crypt_block(block, &reverse(&self.rk))
    }

    /// 加密（自动 PKCS#7 填充）。
    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let padded = pad_pkcs7(plaintext);
        let mut out = Vec::with_capacity(padded.len());
        for chunk in padded.chunks(16) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            out.extend_from_slice(&crypt_block(&block, &self.rk));
        }
        out
    }

    /// 解密（自动去填充）。
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        if ciphertext.len() % 16 != 0 {
            return Err("密文长度非法".to_string());
        }
        let mut out = Vec::with_capacity(ciphertext.len());
        for chunk in ciphertext.chunks(16) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            out.extend_from_slice(&crypt_block(&block, &reverse(&self.rk)));
        }
        unpad_pkcs7(&out)
    }

    /// SM4-CBC 加密（PKCS#7 填充 + 调用方提供的 16 字节 IV）。
    ///
    /// CBC 将每个明文分组与前一密文分组（首组与 IV）异或后再加密，
    /// 消除 ECB「相同明文→相同密文」的缺陷，满足国密合规对语义安全的要求。
    /// IV 必须由随机源生成且与密文一同传输（见 `auth::mobile` 的信封封装）。
    pub fn encrypt_cbc(&self, iv: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
        let padded = pad_pkcs7(plaintext);
        let mut out = Vec::with_capacity(padded.len());
        let mut prev = *iv;
        for chunk in padded.chunks(16) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            for i in 0..16 {
                block[i] ^= prev[i];
            }
            let enc = crypt_block(&block, &self.rk);
            out.extend_from_slice(&enc);
            prev = enc;
        }
        out
    }

    /// SM4-CBC 解密（去除 PKCS#7 填充）。
    pub fn decrypt_cbc(&self, iv: &[u8; 16], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        if ciphertext.len() % 16 != 0 {
            return Err("密文长度非法（非 16 字节整数倍）".to_string());
        }
        let mut out = Vec::with_capacity(ciphertext.len());
        let mut prev = *iv;
        for chunk in ciphertext.chunks(16) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            let dec = crypt_block(&block, &reverse(&self.rk));
            let mut plain_block = [0u8; 16];
            for i in 0..16 {
                plain_block[i] = dec[i] ^ prev[i];
            }
            out.extend_from_slice(&plain_block);
            prev = block;
        }
        unpad_pkcs7(&out)
    }
}

/// 单分组（16 字节）SM4 加密，便于直接套用标准测试向量（KAT）。
pub fn sm4_encrypt(block: &[u8; 16], key: &[u8; 16]) -> [u8; 16] {
    Sm4::new(key).encrypt_block(block)
}

/// 逆序轮密钥用于解密。
fn reverse(rk: &[u32; 32]) -> [u32; 32] {
    let mut out = [0u32; 32];
    for i in 0..32 {
        out[i] = rk[31 - i];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GM/T 0002-2012 附录 A 标准测试向量（KAT）。
    #[test]
    fn sm4_kat_gmt_0002_appendix_a() {
        // 标准测试向量
        let key = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10,
        ];
        let plaintext = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10,
        ];
        let expected = [
            0x68, 0x1e, 0xdf, 0x34, 0xd2, 0x06, 0x96, 0x5e, 0x86, 0xb3, 0xe9, 0x4f, 0x53, 0x6e,
            0x42, 0x46,
        ];
        let cipher = sm4_encrypt(&plaintext, &key);
        assert_eq!(cipher, expected, "SM4 GM/T 0002 附录A 标准向量不匹配");

        // 全零测试向量（密钥/明文全零）。
        // 注意：全零向量的密文与附录 A 示例 1 并不相同，是独立的已知向量。
        let zero_key = [0u8; 16];
        let zero_pt = [0u8; 16];
        let zero_expected = [
            0x9F, 0x1F, 0x7B, 0xFF, 0x6F, 0x55, 0x11, 0x38, 0x4D, 0x94, 0x30, 0x53, 0x1E, 0x53,
            0x8F, 0xD3,
        ];
        assert_eq!(
            sm4_encrypt(&zero_pt, &zero_key),
            zero_expected,
            "SM4 全零测试向量不匹配"
        );
    }

    /// S-box 自检：确认硬编码常量与权威 SM4 S-box（GB/T 32907 表 1）一致。
    #[test]
    fn sm4_sbox_selfcheck() {
        assert_eq!(SM4_SBOX[0x00], 0xD6, "S-box 首元素应为 0xD6");
        assert_eq!(SM4_SBOX[0xFF], 0x48, "S-box 末元素应为 0x48");
        // 原生成式实现在 0x82 / 0xA4 等高索引处出错，这里固定校验权威值：
        // 索引 0x82 → 0x8A；值 0x82 落在索引 0xA4，其相邻索引 0xA5 → 0x66。
        assert_eq!(SM4_SBOX[0x82], 0x8A, "S-box[0x82] 应为 0x8A");
        assert_eq!(SM4_SBOX[0xA4], 0x82, "值 0x82 应位于索引 0xA4");
        assert_eq!(SM4_SBOX[0xA5], 0x66, "值 0x66 应位于索引 0xA5");
        assert_eq!(
            SM4_SBOX[0xEF], 0x84,
            "S-box[0xEF] 应为 0x84（标准文档示例）"
        );
    }

    #[test]
    fn test_sm4_roundtrip() {
        let key = [0x42u8; 16];
        let sm4 = Sm4::new(&key);
        for pt in [
            b"".as_slice(),
            b"hello".as_slice(),
            b"exact 16 bytes!!".as_slice(),
            b"a longer plaintext exceeding one block boundary".as_slice(),
        ] {
            let ct = sm4.encrypt(pt);
            assert_eq!(ct.len() % 16, 0, "密文应为 16 字节整数倍");
            let dt = sm4.decrypt(&ct).expect("解密应成功");
            assert_eq!(dt, pt, "SM4 加解密应可往返");
        }
    }

    #[test]
    fn test_sm4_from_material_deterministic() {
        let a = Sm4::from_material(b"campus-secret-key");
        let b = Sm4::from_material(b"campus-secret-key");
        assert_eq!(a.encrypt(b"x"), b.encrypt(b"x"));
    }

    /// CBC 往返 + 语义安全：相同明文 + 不同 IV 必须产生不同密文。
    #[test]
    fn test_sm4_cbc_roundtrip_and_iv_uniqueness() {
        let key = [0x42u8; 16];
        let sm4 = Sm4::new(&key);
        let pt = b"a longer plaintext exceeding one block boundary for CBC testing";
        let iv1 = [1u8; 16];
        let iv2 = [2u8; 16];
        let c1 = sm4.encrypt_cbc(&iv1, pt);
        let c2 = sm4.encrypt_cbc(&iv2, pt);
        assert_ne!(c1, c2, "CBC 下不同 IV 应产生不同密文（语义安全）");
        assert_eq!(c1.len(), c2.len());
        assert_eq!(sm4.decrypt_cbc(&iv1, &c1).unwrap(), pt, "CBC 解密应往返");
        assert_eq!(sm4.decrypt_cbc(&iv2, &c2).unwrap(), pt, "CBC 解密应往返");
        // 注：CBC 中错误 IV 仅破坏首个明文分组，末组 PKCS#7 填充仍可能合法，
        // 因此不会必然报错；「错误密钥→解密失败」由 auth::mobile 的信封测试覆盖。
    }
}
