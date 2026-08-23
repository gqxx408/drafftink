import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import { useToast } from '../components/Toast';
import { ApiRequestError, applySeal } from '../api/client';
import type { WorkflowInstance } from '../api/types';

const SEAL_TYPES = ['公章', '财务专用章', '合同专用章', '法人章', '发票专用章'];

export default function Seal() {
  const toast = useToast();
  const nav = useNavigate();
  const [title, setTitle] = useState('');
  const [docTitle, setDocTitle] = useState('');
  const [sealType, setSealType] = useState(SEAL_TYPES[0]);
  const [reason, setReason] = useState('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<WorkflowInstance | null>(null);
  const [advice, setAdvice] = useState('');

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim() || !docTitle.trim() || !reason.trim()) {
      toast('请完整填写用印申请信息');
      return;
    }
    setBusy(true);
    try {
      const r = await applySeal({
        title: title.trim(),
        doc_title: docTitle.trim(),
        seal_type: sealType,
        reason: reason.trim(),
      });
      setResult(r.workflow);
      setAdvice(r.ai_advice);
      toast('用印申请已提交');
    } catch (e) {
      toast(e instanceof ApiRequestError ? e.message : '提交失败');
    } finally {
      setBusy(false);
    }
  }

  if (result) {
    return (
      <Layout title="用印申请" subtitle="提交成功">
        <div className="card">
          <h3>✅ 用印申请已提交</h3>
          <div className="meta">审批流：部门负责人 → 校办（或签）</div>
        </div>
        {advice && (
          <div className="advice" style={{ marginBottom: 14 }}>
            🤖 AI 办公助理建议：\n{advice}
          </div>
        )}
        <button className="btn" onClick={() => nav('/approvals')}>
          查看我的待办
        </button>
        <div style={{ height: 10 }} />
        <button className="btn ghost" onClick={() => nav('/')}>
          返回工作台
        </button>
      </Layout>
    );
  }

  return (
    <Layout title="用印申请" subtitle="Seal 工作流">
      <form onSubmit={submit}>
        <div className="field">
          <label>申请标题</label>
          <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="如：2026招生简章用印" />
        </div>
        <div className="field">
          <label>用印文件名称</label>
          <input className="input" value={docTitle} onChange={(e) => setDocTitle(e.target.value)} placeholder="如：2026年招生简章（终稿）" />
        </div>
        <div className="field">
          <label>用印类型</label>
          <select className="select" value={sealType} onChange={(e) => setSealType(e.target.value)}>
            {SEAL_TYPES.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label>用印事由</label>
          <textarea className="textarea" value={reason} onChange={(e) => setReason(e.target.value)} placeholder="说明用印用途与依据…" />
        </div>
        <button className="btn" disabled={busy}>
          {busy ? '提交中…' : '提 交 申 请'}
        </button>
      </form>
      <div style={{ height: 10 }} />
      <button className="btn ghost" onClick={() => nav('/')}>
        返回工作台
      </button>
    </Layout>
  );
}
