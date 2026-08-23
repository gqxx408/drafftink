// 与后端 drafftink-backend 移动办公 REST 接口对应的前端类型定义。
// 字段命名与后端 serde 序列化保持一致（snake_case）。

export type Role = 'admin' | 'teacher' | 'student';

export interface UserInfo {
  id: string;
  username: string;
  display_name: string;
  role: Role;
  class_id: string | null;
  tenant_id: string;
}

export interface LoginResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in: number;
  mfa_required: boolean;
  user: UserInfo;
}

export interface MfaVerifyResponse {
  verified: boolean;
  sso_token: string;
}

export interface SsoTokenResponse {
  sso_token: string;
}

export type WorkflowType = 'official_doc' | 'seal' | 'vehicle';
export type WorkflowStatus =
  | 'draft'
  | 'in_progress'
  | 'approved'
  | 'rejected'
  | 'withdrawn';
export type ApprovalMode = 'counter_sign' | 'or_sign';
export type ApprovalDecision = 'approve' | 'reject';

export interface WorkflowNode {
  name: string;
  roles: Role[];
  mode: ApprovalMode;
}

export interface ApprovalRecord {
  node_index: number;
  node_name: string;
  approver_id: string;
  approver_name: string;
  approver_role: Role;
  decision: ApprovalDecision;
  comment: string;
  at: string;
}

export interface OfficialDoc {
  doc_id?: string;
  title?: string;
  doc_type?: string;
  doc_title?: string;
  issue_date?: string;
  issue_dept?: string;
  urgency?: string;
  secret_level?: string;
  approval_status?: string;
  [key: string]: unknown;
}

export interface WorkflowInstance {
  id: string;
  workflow_type: WorkflowType;
  title: string;
  applicant_id: string;
  applicant_name: string;
  tenant_id: string;
  status: WorkflowStatus;
  nodes: WorkflowNode[];
  current_node: number;
  approvals: ApprovalRecord[];
  created_at: string;
  updated_at: string;
  official_doc: OfficialDoc | null;
  payload: Record<string, unknown>;
  // 部分接口会附带 AI 顾问建议
  ai_advice?: string;
}

export interface Announcement {
  notice_id: string;
  title: string;
  publish_date: string;
  publisher: string;
  recv_scope: string;
  body: string;
  tenant_id: string;
  pinned: boolean;
}

export interface MeetingBooking {
  id: string;
  title: string;
  organizer_id: string;
  organizer_name: string;
  start_time: string;
  end_time: string;
  location: string;
  participants: string;
  tenant_id: string;
  created_at: string;
}

export interface MessageView {
  id: string;
  title: string;
  channel: string;
  created_at: string;
  read: boolean;
  encrypted_body: string;
}

// 通用后端错误体（AppError 序列化为 { error: string }）
export interface ApiError {
  error: string;
}
