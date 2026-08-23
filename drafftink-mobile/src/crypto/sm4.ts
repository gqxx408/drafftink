import { sm4 } from 'sm-crypto';
import { SM4_SECRET } from '../config';
import { getDeviceFp } from '../api/client';

// 由设备指纹与校内共享密钥派生 16 字节 SM4 密钥：
//   key = SHA-256(device_fp ‖ SM4_SECRET)[0..16]
// 与后端 auth::mobile::derive_sm4_key 完全一致（GB/T 32907-2016）。
async function deriveSm4Key(): Promise<number[]> {
  const material = getDeviceFp() + SM4_SECRET;
  const data = new TextEncoder().encode(material);
  const digest = await crypto.subtle.digest('SHA-256', data);
  const bytes = new Uint8Array(digest).slice(0, 16);
  return Array.from(bytes);
}

function base64ToBytes(b64: string): number[] {
  const bin = atob(b64);
  const out: number[] = [];
  for (let i = 0; i < bin.length; i++) out.push(bin.charCodeAt(i));
  return out;
}

/**
 * 解密后端以 SM4(ECB + PKCS#7) Base64 编码的密文（消息正文信封）。
 * 解密在本地完成，明文不离开设备；密钥由设备指纹与预置校内密钥派生。
 */
export async function decryptSm4Text(cipherB64: string): Promise<string> {
  try {
    const key = await deriveSm4Key();
    const cipherBytes = base64ToBytes(cipherB64);
    // sm-crypto 接受字节数组作为原始密文（无需 input 解码），按 ECB + PKCS#7 解密。
    const plain = sm4.decrypt(cipherBytes, key, {
      mode: 'ecb',
      padding: 'pkcs#7',
    });
    return typeof plain === 'string' ? plain : String(plain);
  } catch (e) {
    console.error('SM4 解密失败', e);
    return '（消息正文解密失败）';
  }
}
