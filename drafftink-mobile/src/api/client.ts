import {
  ACCESS_TOKEN_KEY,
  API_BASE,
  DEVICE_FP_KEY,
  REFRESH_TOKEN_KEY,
  USER_KEY,
} from '../config';
import type {
  Announcement,
  LoginResponse,
  MeetingBooking,
  MessageView,
  MfaVerifyResponse,
  SsoTokenResponse,
  UserInfo,
  WorkflowInstance,
  WorkflowType,
} from './types';

// ── 设备指纹 ───────────────────────────────────────────────────────────────
// 稳定的设备指纹：首次启动时生成并持久化于 localStorage，后续所有请求携带，
// 以便后端将访问令牌与设备绑定（防令牌盗用）。
export function getDeviceFp(): string {
  let fp = localStorage.getItem(DEVICE_FP_KEY);
  if (!fp) {
    fp =
      (crypto as Crypto & { randomUUID?: () => string }).randomUUID?.() ||
      'dev-' + Math.random().toString(36).slice(2) + Date.now().toString(36);
    localStorage.setItem(DEVICE_FP_KEY, fp);
  }
  return fp;
}

// ── 令牌与用户态持久化 ──────────────────────────────────────────────────────
export function saveSession(resp: LoginResponse) {
  localStorage.setItem(ACCESS_TOKEN_KEY, resp.access_token);
  localStorage.setItem(REFRESH_TOKEN_KEY, resp.refresh_token);
  localStorage.setItem(USER_KEY, JSON.stringify(resp.user));
}

export function getAccessToken(): string | null {
  return localStorage.getItem(ACCESS_TOKEN_KEY);
}

export function getRefreshToken(): string | null {
  return localStorage.getItem(REFRESH_TOKEN_KEY);
}

export function getStoredUser(): UserInfo | null {
  const raw = localStorage.getItem(USER_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as UserInfo;
  } catch {
    return null;
  }
}

export function clearSession() {
  localStorage.removeItem(ACCESS_TOKEN_KEY);
  localStorage.removeItem(REFRESH_TOKEN_KEY);
  localStorage.removeItem(USER_KEY);
}

// ── 底层请求封装 ───────────────────────────────────────────────────────────
export class ApiRequestError extends Error {
  status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = 'ApiRequestError';
    this.status = status;
  }
}

interface RequestOptions {
  method?: string;
  body?: unknown;
  // 跳过自动附加访问令牌（如登录接口本身）
  auth?: boolean;
  // 原始响应（不解析 JSON），用于刷新接口等
  raw?: boolean;
}

async function request<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'x-device-fp': getDeviceFp(),
  };
  if (opts.auth !== false) {
    const token = getAccessToken();
    if (token) headers['Authorization'] = `Bearer ${token}`;
  }
  const init: RequestInit = {
    method: opts.method || 'GET',
    headers,
  };
  if (opts.body !== undefined) {
    init.body = JSON.stringify(opts.body);
  }

  const res = await fetch(`${API_BASE}${path}`, init);
  if (!res.ok) {
    let msg = `请求失败 (${res.status})`;
    try {
      const data = (await res.json()) as { error?: string };
      if (data?.error) msg = data.error;
    } catch {
      /* ignore */
    }
    throw new ApiRequestError(msg, res.status);
  }
  if (opts.raw) return undefined as T;
  const text = await res.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}

// ── 移动办公接口 ───────────────────────────────────────────────────────────

/** 登录：校验凭证 → 返回访问/刷新令牌（已绑定设备指纹）→ 下发短信验证码。 */
export function login(username: string, password: string) {
  return request<LoginResponse>('/api/mobile/login', {
    method: 'POST',
    auth: false,
    body: { username, password, device_fp: getDeviceFp() },
  });
}

/** 演示用：回显当前用户的短信验证码（仅后端开发模式 DRAFTTINK_DEV_MODE=true 可用）。 */
export function fetchDevSmsCode(accessToken: string) {
  return request<{ code: string | null }>('/api/mobile/mfa/dev-code', {
    method: 'POST',
    auth: false,
    body: { access_token: accessToken },
  });
}

/** 短信二次验证：成功后返回校园级 SSO 令牌（GB/T 36342-2018）。 */
export function mfaVerify(accessToken: string, smsCode: string) {
  return request<MfaVerifyResponse>('/api/mobile/mfa/verify', {
    method: 'POST',
    auth: false,
    body: { access_token: accessToken, sms_code: smsCode },
  });
}

/** 取回已签发的 SSO 令牌。 */
export function ssoToken() {
  return request<SsoTokenResponse>('/api/mobile/sso/token');
}

/** 当前角色的待办审批。 */
export function listTodos() {
  return request<WorkflowInstance[]>('/api/mobile/todos');
}

/** 发起审批（公文 / 用印 / 车辆）。仅教师/管理员可发起。 */
export function startWorkflow(workflowType: WorkflowType, title: string, payload: unknown) {
  return request<{ workflow: WorkflowInstance; ai_advice: string }>(
    '/api/mobile/workflow/start',
    { method: 'POST', body: { workflow_type: workflowType, title, payload } },
  );
}

/** 审批详情。 */
export function getWorkflow(id: string) {
  return request<WorkflowInstance>(`/api/mobile/workflow/${id}`);
}

/** 提交审批决定（会签/或签 + RBAC）。 */
export function approveWorkflow(id: string, decision: 'approve' | 'reject', comment = '') {
  return request<{ workflow: WorkflowInstance; ai_advice: string }>(
    '/api/mobile/workflow/approve',
    { method: 'POST', body: { workflow_id: id, decision, comment } },
  );
}

/** 通知公告（ZXBG0201）。 */
export function listAnnouncements() {
  return request<Announcement[]>('/api/mobile/announcements');
}

/** 会议预约。 */
export function bookMeeting(input: {
  title: string;
  start_time: string;
  end_time: string;
  location: string;
  participants?: string;
}) {
  return request<MeetingBooking>('/api/mobile/meeting/book', {
    method: 'POST',
    body: input,
  });
}

/** 用印申请（Seal 工作流）。 */
export function applySeal(input: {
  title: string;
  doc_title: string;
  seal_type: string;
  reason: string;
}) {
  return request<{ workflow: WorkflowInstance; ai_advice: string }>(
    '/api/mobile/seal/apply',
    { method: 'POST', body: input },
  );
}

/** 消息中心：敏感正文以 SM4 信封加密（GB/T 32907-2016）。 */
export function listMessages() {
  return request<MessageView[]>('/api/mobile/messages');
}
