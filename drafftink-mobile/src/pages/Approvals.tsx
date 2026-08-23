import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import { useAuth } from '../auth/AuthContext';
import { useToast } from '../components/Toast';
import { ApiRequestError, approveWorkflow, listTodos } from '../api/client';
import type { ApprovalRecord, WorkflowInstance } from '../api/types';
import {
  formatDateTime,
  modeLabel,
  roleLabel,
  statusLabel,
  workflowTypeLabel,
} from '../utils/labels';

export default function Approvals() {
  const { user } = useAuth();
  const toast = useToast();
  const nav = useNavigate();
  const [items, setItems] = useState<WorkflowInstance[]>([]);
  const [selected, setSelected] = useState<WorkflowInstance | null>(null);
  const [comment, setComment] = useState('');
  const [busy, setBusy] = useState(false);

  const load = () => listTodos().then(setItems).catch(() => setItems([]));

  useEffect(() => {
    load();
  }, []);

  // 当前用户是否可审批所选实例的当前节点
  const canApprove = useMemo(() => {
    if (!selected || !user) return false;
    if (selected.status !== 'in_progress') return false;
    const node = selected.nodes[selected.current_node];
    return !!node && node.roles.includes(user.role);
  }, [selected, user]);

  async function act(decision: 'approve' | 'reject') {
    if (!selected) return;
    setBusy(true);
    try {
      const r = await approveWorkflow(selected.id, decision, comment);
      toast(decision === 'approve' ? '已同意' : '已驳回');
      setSelected(r.workflow);
      await load();
      if (r.ai_advice) toast('AI 建议已更新');
    } catch (e) {
      toast(e instanceof ApiRequestError ? e.message : '操作失败');
    } finally {
      setBusy(false);
      setComment('');
    }
  }

  return (
    <Layout title="待办审批" subtitle="公文 / 用印 / 车辆">
      {items.length === 0 && <div className="empty">暂无待办审批 🎉</div>}

      {!selected &&
        items.map((w) => {
          const st = statusLabel(w.status);
          return (
            <div key={w.id} className="list-item" onClick={() => setSelected(w)}>
              <div className="row" style={{ justifyContent: 'space-between' }}>
                <div className="title">{w.title}</div>
                <span className={`badge ${st.cls}`}>{st.text}</span>
              </div>
              <div className="meta">
                {workflowTypeLabel(w.workflow_type)} · 发起人 {w.applicant_name} ·{' '}
                {formatDateTime(w.created_at)}
              </div>
            </div>
          );
        })}

      {selected && (
        <div>
          <button className="btn ghost" style={{ width: 'auto', marginBottom: 12 }} onClick={() => setSelected(null)}>
            ← 返回列表
          </button>

          <div className="card">
            <div className="row" style={{ justifyContent: 'space-between' }}>
              <h3 style={{ margin: 0 }}>{selected.title}</h3>
              <span className={`badge ${statusLabel(selected.status).cls}`}>
                {statusLabel(selected.status).text}
              </span>
            </div>
            <div className="meta" style={{ marginTop: 6 }}>
              {workflowTypeLabel(selected.workflow_type)} · 发起人 {selected.applicant_name}
            </div>

            {/* 审批节点时间线 */}
            <div style={{ marginTop: 14 }}>
              {selected.nodes.map((n, i) => {
                const recs = selected.approvals.filter((a) => a.node_index === i);
                const done = recs.length > 0;
                const rejected = recs.some((r) => r.decision === 'reject');
                const cls = rejected ? 'rejected' : done ? 'done' : i === selected.current_node ? 'current' : '';
                return (
                  <div className="node-step" key={i}>
                    <div className={`node-dot ${cls}`}>{done ? (rejected ? '✕' : '✓') : i + 1}</div>
                    <div style={{ flex: 1 }}>
                      <div style={{ fontWeight: 600, fontSize: 14 }}>
                        {n.name} <span className="muted" style={{ fontSize: 12 }}>({modeLabel(n.mode)})</span>
                      </div>
                      <div className="meta">
                        审批角色：{n.roles.map(roleLabel).join(' / ')}
                      </div>
                      {recs.map((r: ApprovalRecord) => (
                        <div key={r.approver_id + r.at} className="meta" style={{ marginTop: 4 }}>
                          {r.approver_name}（{roleLabel(r.approver_role)}）·
                          {r.decision === 'approve' ? '同意' : '驳回'}
                          {r.comment ? `：${r.comment}` : ''}
                        </div>
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {selected.ai_advice && (
            <div className="advice" style={{ marginBottom: 14 }}>
              🤖 AI 办公助理建议：\n{selected.ai_advice}
            </div>
          )}

          {canApprove && (
            <div className="card">
              <div className="field">
                <label>审批意见（可选）</label>
                <textarea
                  className="textarea"
                  value={comment}
                  onChange={(e) => setComment(e.target.value)}
                  placeholder="填写审批意见…"
                />
              </div>
              <div className="btn-row">
                <button className="btn success" disabled={busy} onClick={() => act('approve')}>
                  同 意
                </button>
                <button className="btn danger" disabled={busy} onClick={() => act('reject')}>
                  驳 回
                </button>
              </div>
            </div>
          )}

          {!canApprove && selected.status === 'in_progress' && (
            <div className="hint">当前节点非您的角色，无法审批（RBAC 校验）。</div>
          )}
        </div>
      )}

      <div style={{ height: 8 }} />
      <button className="btn ghost" onClick={() => nav('/')}>
        返回工作台
      </button>
    </Layout>
  );
}
