import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import { listAnnouncements, listMessages, listTodos } from '../api/client';
import type { Announcement, MessageView, WorkflowInstance } from '../api/types';
import { statusLabel, workflowTypeLabel } from '../utils/labels';

export default function AIDashboard() {
  const nav = useNavigate();
  const [todos, setTodos] = useState<WorkflowInstance[]>([]);
  const [ann, setAnn] = useState<Announcement[]>([]);
  const [msgs, setMsgs] = useState<MessageView[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([listTodos(), listAnnouncements(), listMessages()])
      .then(([t, a, m]) => {
        setTodos(t);
        setAnn(a);
        setMsgs(m);
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  // 按状态聚合（含历史，这里以当前用户可见待办近似）
  const stats = useMemo(() => {
    const byStatus: Record<string, number> = {};
    const byType: Record<string, number> = {};
    for (const w of todos) {
      byStatus[w.status] = (byStatus[w.status] ?? 0) + 1;
      byType[w.workflow_type] = (byType[w.workflow_type] ?? 0) + 1;
    }
    return { byStatus, byType };
  }, [todos]);

  // 离线 AI 启发式洞察（不依赖外部模型，数据不出校）
  const insight = useMemo(() => {
    const lines: string[] = [];
    const pending = todos.filter((t) => t.status === 'in_progress').length;
    const approved = todos.filter((t) => t.status === 'approved').length;
    const rejected = todos.filter((t) => t.status === 'rejected').length;
    if (pending === 0) lines.push('当前无在途审批，办公流处于空闲状态。');
    else lines.push(`有 ${pending} 项审批在途，建议优先处理即将到期的公文与用印申请。`);
    if (approved > 0) lines.push(`本周期已完成 ${approved} 项审批，归档规范。`);
    if (rejected > 0) lines.push(`有 ${rejected} 项被驳回，建议回访申请人确认材料完整性。`);
    if (ann.length === 0) lines.push('暂无最新通知公告，可提醒校办补充发布。');
    else lines.push(`已发布 ${ann.length} 条通知公告，覆盖全体教职工。`);
    return lines.join('\n');
  }, [todos, ann]);

  const maxType = Math.max(1, ...Object.values(stats.byType));

  if (loading) {
    return (
      <Layout title="AI 学情看板" subtitle="智能办公洞察">
        <div className="empty">加载中…</div>
      </Layout>
    );
  }

  return (
    <Layout title="AI 学情看板" subtitle="智能办公洞察（离线）">
      <div className="card">
        <h3>📊 办公概览</h3>
        <div className="row" style={{ justifyContent: 'space-between', marginTop: 8 }}>
          <div className="stat">
            <div className="num">{todos.length}</div>
            <div className="lbl">审批实例</div>
          </div>
          <div className="stat">
            <div className="num">{ann.length}</div>
            <div className="lbl">通知公告</div>
          </div>
          <div className="stat">
            <div className="num">{msgs.length}</div>
            <div className="lbl">消息</div>
          </div>
        </div>
      </div>

      <div className="card">
        <h3>审批类型分布</h3>
        {Object.keys(stats.byType).length === 0 && <div className="muted">暂无数据</div>}
        {Object.entries(stats.byType).map(([t, n]) => (
          <div key={t} style={{ marginBottom: 10 }}>
            <div className="meta" style={{ marginBottom: 4 }}>
              {workflowTypeLabel(t as never)} · {n}
            </div>
            <div style={{ background: '#eef0f4', borderRadius: 6, height: 10 }}>
              <div
                style={{
                  width: `${(n / maxType) * 100}%`,
                  background: 'var(--primary)',
                  height: 10,
                  borderRadius: 6,
                }}
              />
            </div>
          </div>
        ))}
      </div>

      <div className="card">
        <h3>状态分布</h3>
        <div className="row" style={{ flexWrap: 'wrap', gap: 8 }}>
          {(['in_progress', 'approved', 'rejected', 'draft', 'withdrawn'] as const).map((s) => (
            <span key={s} className={`badge ${statusLabel(s).cls}`}>
              {statusLabel(s).text}: {stats.byStatus[s] ?? 0}
            </span>
          ))}
        </div>
      </div>

      <div className="advice">
        🤖 AI 办公助理洞察：\n{insight}
      </div>

      <div style={{ height: 10 }} />
      <button className="btn ghost" onClick={() => nav('/')}>
        返回工作台
      </button>
    </Layout>
  );
}
