import type { ApprovalMode, Role, WorkflowStatus, WorkflowType } from '../api/types';

export function workflowTypeLabel(t: WorkflowType): string {
  switch (t) {
    case 'official_doc':
      return '公文流转';
    case 'seal':
      return '用印申请';
    case 'vehicle':
      return '车辆审批';
  }
}

export function statusLabel(s: WorkflowStatus): { text: string; cls: string } {
  switch (s) {
    case 'draft':
      return { text: '草稿', cls: 'gray' };
    case 'in_progress':
      return { text: '审批中', cls: 'blue' };
    case 'approved':
      return { text: '已通过', cls: 'green' };
    case 'rejected':
      return { text: '已驳回', cls: 'red' };
    case 'withdrawn':
      return { text: '已撤回', cls: 'amber' };
  }
}

export function modeLabel(m: ApprovalMode): string {
  return m === 'counter_sign' ? '会签' : '或签';
}

export function roleLabel(r: Role): string {
  switch (r) {
    case 'admin':
      return '管理员';
    case 'teacher':
      return '教师';
    case 'student':
      return '学生';
  }
}

export function formatDateTime(s: string): string {
  const d = new Date(s);
  if (isNaN(d.getTime())) return s;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}`;
}

// 将本地时间输入框的 value（YYYY-MM-DDTHH:mm）转为 RFC3339（UTC Z）
export function localToRfc3339(local: string): string {
  const d = new Date(local);
  return d.toISOString();
}
