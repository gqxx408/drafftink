import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import { useToast } from '../components/Toast';
import { ApiRequestError, bookMeeting } from '../api/client';
import { localToRfc3339 } from '../utils/labels';

function nowLocal(offsetMin = 0): string {
  const d = new Date(Date.now() + offsetMin * 60000);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(
    d.getMinutes(),
  )}`;
}

export default function Meeting() {
  const toast = useToast();
  const nav = useNavigate();
  const [title, setTitle] = useState('');
  const [start, setStart] = useState(nowLocal(60));
  const [end, setEnd] = useState(nowLocal(120));
  const [location, setLocation] = useState('');
  const [participants, setParticipants] = useState('');
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim() || !location.trim()) {
      toast('请填写会议主题与地点');
      return;
    }
    if (new Date(end) <= new Date(start)) {
      toast('结束时间须晚于开始时间');
      return;
    }
    setBusy(true);
    try {
      await bookMeeting({
        title: title.trim(),
        start_time: localToRfc3339(start),
        end_time: localToRfc3339(end),
        location: location.trim(),
        participants: participants.trim(),
      });
      setDone(true);
      toast('会议预约成功');
    } catch (e) {
      toast(e instanceof ApiRequestError ? e.message : '预约失败');
    } finally {
      setBusy(false);
    }
  }

  if (done) {
    return (
      <Layout title="会议预约" subtitle="提交成功">
        <div className="card">
          <h3>✅ 会议已预约</h3>
          <div className="meta">可在后台直播/录播资源管理平台关联会议资料。</div>
        </div>
        <button className="btn" onClick={() => nav('/')}>
          返回工作台
        </button>
      </Layout>
    );
  }

  return (
    <Layout title="会议预约" subtitle="在线订场">
      <form onSubmit={submit}>
        <div className="field">
          <label>会议主题</label>
          <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="如：初三一模考务会" />
        </div>
        <div className="field">
          <label>开始时间</label>
          <input className="input" type="datetime-local" value={start} onChange={(e) => setStart(e.target.value)} />
        </div>
        <div className="field">
          <label>结束时间</label>
          <input className="input" type="datetime-local" value={end} onChange={(e) => setEnd(e.target.value)} />
        </div>
        <div className="field">
          <label>地点</label>
          <input className="input" value={location} onChange={(e) => setLocation(e.target.value)} placeholder="如：行政楼301" />
        </div>
        <div className="field">
          <label>参会人（选填，逗号分隔）</label>
          <input className="input" value={participants} onChange={(e) => setParticipants(e.target.value)} placeholder="如：王老师,李老师" />
        </div>
        <button className="btn" disabled={busy}>
          {busy ? '提交中…' : '预 约 会 议'}
        </button>
      </form>
      <div style={{ height: 10 }} />
      <button className="btn ghost" onClick={() => nav('/')}>
        返回工作台
      </button>
    </Layout>
  );
}
