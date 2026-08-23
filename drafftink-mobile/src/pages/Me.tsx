import { useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import { useAuth } from '../auth/AuthContext';
import { roleLabel } from '../utils/labels';

const LINKS = [
  { to: '/meeting', icon: '📅', name: '会议预约' },
  { to: '/seal', icon: '🔖', name: '用印申请' },
  { to: '/messages', icon: '💬', name: '消息中心' },
  { to: '/ai', icon: '🤖', name: 'AI 学情看板' },
];

export default function Me() {
  const { user, logout } = useAuth();
  const nav = useNavigate();

  return (
    <Layout title="我的" subtitle="个人与设置">
      <div className="card">
        <div className="row" style={{ gap: 14 }}>
          <div className="avatar" style={{ width: 52, height: 52, background: 'var(--primary)', color: '#fff', fontSize: 22 }}>
            {(user?.display_name || '?').slice(0, 1)}
          </div>
          <div>
            <div style={{ fontWeight: 700, fontSize: 17 }}>{user?.display_name}</div>
            <div className="meta">
              {roleLabel(user?.role ?? 'student')} · @{user?.username}
            </div>
          </div>
        </div>
        <div className="meta" style={{ marginTop: 12 }}>
          租户/学校 ID：<code>{user?.tenant_id}</code>
        </div>
      </div>

      <div className="section-title">更多办公</div>
      <div className="grid">
        {LINKS.map((l) => (
          <button key={l.to} className="feature" onClick={() => nav(l.to)}>
            <span className="ico">{l.icon}</span>
            <span className="name">{l.name}</span>
          </button>
        ))}
      </div>

      <div style={{ height: 14 }} />
      <button className="btn danger" onClick={logout}>
        退出登录
      </button>
    </Layout>
  );
}
