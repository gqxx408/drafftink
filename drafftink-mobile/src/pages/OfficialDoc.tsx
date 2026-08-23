import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import { useToast } from '../components/Toast';
import { ApiRequestError, startWorkflow } from '../api/client';
import type { WorkflowInstance } from '../api/types';

const DOC_TYPES = [
  ['80', '通知'],
  ['20', '决定'],
  ['70', '意见'],
  ['50', '公告'],
  ['60', '通告'],
  ['90', '报告'],
  ['91', '请示'],
  ['92', '批复'],
  ['10', '决议'],
  ['99', '其他'],
];
const URGENCY = [
  ['9', '普通'],
  ['3', '平急'],
  ['2', '加急'],
  ['1', '特急'],
];
const SECRET = [
  ['0', '非涉密'],
  ['1', '秘密'],
  ['2', '机密'],
  ['3', '绝密'],
];

function todayYmd(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}`;
}

export default function OfficialDoc() {
  const toast = useToast();
  const nav = useNavigate();
  const [title, setTitle] = useState('');
  const [docType, setDocType] = useState('80');
  const [issueDate, setIssueDate] = useState(todayYmd());
  const [issueDept, setIssueDept] = useState('');
  const [urgency, setUrgency] = useState('9');
  const [secret, setSecret] = useState('0');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<WorkflowInstance | null>(null);
  const [advice, setAdvice] = useState('');

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim() || !issueDept.trim()) {
      toast('请填写公文标题与发文部门');
      return;
    }
    setBusy(true);
    try {
      const r = await startWorkflow('official_doc', title.trim(), {
        doc_type: docType,
        issue_date: issueDate,
        issue_dept: issueDept.trim(),
        urgency,
        secret_level: secret,
      });
      setResult(r.workflow);
      setAdvice(r.ai_advice);
      toast('公文已提交，进入审批流');
    } catch (e) {
      toast(e instanceof ApiRequestError ? e.message : '提交失败');
    } finally {
      setBusy(false);
    }
  }

  if (result) {
    return (
      <Layout title="公文流转" subtitle="提交成功">
        <div className="card">
          <h3>✅ 已提交审批</h3>
          <div className="meta">公文编号：{result.official_doc?.doc_id ?? result.id}</div>
          <div className="meta" style={{ marginTop: 6 }}>
            当前状态：审批中（部门负责人 → 分管校领导）
          </div>
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
    <Layout title="公文流转" subtitle="发起公文审批">
      <form onSubmit={submit}>
        <div className="field">
          <label>公文标题</label>
          <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="如：关于2026年春季开学工作的通知" />
        </div>
        <div className="field">
          <label>公文类型（JY/T 1001）</label>
          <select className="select" value={docType} onChange={(e) => setDocType(e.target.value)}>
            {DOC_TYPES.map(([v, n]) => (
              <option key={v} value={v}>
                {v} · {n}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label>发文日期（YYYYMMDD）</label>
          <input className="input" value={issueDate} onChange={(e) => setIssueDate(e.target.value)} placeholder="20260210" />
        </div>
        <div className="field">
          <label>发文部门</label>
          <input className="input" value={issueDept} onChange={(e) => setIssueDept(e.target.value)} placeholder="如：教务处" />
        </div>
        <div className="field">
          <label>紧急程度</label>
          <select className="select" value={urgency} onChange={(e) => setUrgency(e.target.value)}>
            {URGENCY.map(([v, n]) => (
              <option key={v} value={v}>
                {v} · {n}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label>密级</label>
          <select className="select" value={secret} onChange={(e) => setSecret(e.target.value)}>
            {SECRET.map(([v, n]) => (
              <option key={v} value={v}>
                {v} · {n}
              </option>
            ))}
          </select>
        </div>
        <button className="btn" disabled={busy}>
          {busy ? '提交中…' : '提 交 审 批'}
        </button>
      </form>
      <div style={{ height: 10 }} />
      <button className="btn ghost" onClick={() => nav('/')}>
        返回工作台
      </button>
    </Layout>
  );
}
