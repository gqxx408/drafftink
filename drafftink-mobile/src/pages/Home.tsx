import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import { useAuth } from '../auth/AuthContext';
import { listAnnouncements, listMessages, listTodos } from '../api/client';
import type { Announcement, MessageView, WorkflowInstance } from '../api/types';

const FEATURES = [
  { to: '/approvals', icon: '✅', name: '待办审批', desc: '公文/用印/车辆' },
  { to: '/official-doc', icon: '📄', name: '公文流转', desc: '发起公文审批' },
  { to: '/announcements', icon: '📢', name: '通知公告', desc: 'ZXBG0201' },
  { to: '/meeting', icon: '📅', name: '会议预约', desc: '在线订场' },
  { to: '/seal', icon: '🔖', name: '用印申请', desc: '公章/财务章' },
  { to: '/messages', icon: '💬', name: '消息中心', desc: 'SM4 加密' },
  { to: '/ai', icon: '🤖', name: 'AI 学情看板', desc: '离线分析' },
];

export default function Home() {
  const { user } = useAuth();
  const nav = useNavigate();
  const [todos, setTodos] = useState<WorkflowInstance[]>([]);
  const [ann, setAnn] = useState<Announcement[]>([]);
  const [msgs, setMsgs] = useState<MessageView[]>([]);

  useEffect(() => {
    listTodos().then(setTodos).catch(() => {});
    listAnnouncements().then(setAnn).catch(() => {});
    listMessages().then(setMsgs).catch(() => {});
  }, []);

  const greeting = (() => {
    const h = new Date().getHours();
    if (h < 6) return '夜深了';
    if (h < 12) return '早上好';
    if (h < 14) return '中午好';
    if (h < 18) return '下午好';
    return '晚上好';
  })();

  const unread = msgs.filter((m) => !m.read).length;

  return (
    <Layout title="工作台" subtitle={`${greeting}，${user?.display_name ?? ''}`}>
      <div className="card">
        <div className="row" style={{ justifyContent: 'space-between' }}>
          <div className="stat">
            <div className="num">{todos.length}</div>
            <div className="lbl">待办审批</div>
          </div>
          <div className="stat">
            <div className="num">{ann.length}</div>
            <div className="lbl">通知公告</div>
          </div>
          <div className="stat">
            <div className="num">{unread}</div>
            <div className="lbl">未读消息</div>
          </div>
        </div>
      </div>

      <div className="section-title">办公应用</div>
      <div className="grid">
        {FEATURES.map((f) => (
          <button key={f.to} className="feature" onClick={() => nav(f.to)}>
            <span className="ico">{f.icon}</span>
            <span className="name">{f.name}</span>
            <span className="desc">{f.desc}</span>
          </button>
        ))}
      </div>

      <div className="hint" style={{ marginTop: 16 }}>
        全部接口已对接后端移动办公 REST（JWT + 设备指纹绑定 + MFA 二次验证 + SM4 信封加密），
        数据不出校。
      </div>
    </Layout>
  );
}
