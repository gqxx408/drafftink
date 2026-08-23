// 运行时配置：统一从 Vite 注入的环境变量读取，便于前后端解耦部署。

// 后端基础地址（含协议与端口）。开发期默认 http://localhost:8080（后端 CORS 允许任意来源）。
export const API_BASE: string = import.meta.env.VITE_API_BASE || 'http://localhost:8080';

// 校内共享信封密钥（与后端 DRAFTTINK_JWT_SECRET 一致），用于本地派生 SM4 密钥解密消息正文。
// 这是"预置到内部应用的信封密钥"模式，数据不出校。
export const SM4_SECRET: string =
  import.meta.env.VITE_SM4_SECRET || 'drafftink-backend-default-secret';

// 设备指纹本地存储键
export const DEVICE_FP_KEY = 'drafftink_device_fp';
export const ACCESS_TOKEN_KEY = 'drafftink_access_token';
export const REFRESH_TOKEN_KEY = 'drafftink_refresh_token';
export const USER_KEY = 'drafftink_user';
