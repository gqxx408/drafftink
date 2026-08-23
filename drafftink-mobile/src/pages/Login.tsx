import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '../auth/AuthContext';
import { ApiRequestError } from '../api/client';
import { useToast } from '../components/Toast';

const DEMO = [
  { role: '管理员', username: 'admin', password: 'admin123' },
  { role: '教师', username: 'teacher01', password: 'teacher123' },
  { role: '学生', username: 'student01', password: 'student123' },
];

export default function Login() {
  const { login } = useAuth();
  const nav = useNavigate();
  const toast = useToast();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState('');

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErr('');
    try {
      const resp = await login(username.trim(), password);
      toast(resp.mfa_required ? '已发送短信验证码，请完成二次验证' : '登录成功');
      nav('/mfa');
    } catch (e) {
      setErr(e instanceof ApiRequestError ? e.message : '登录失败，请稍后重试');
    } finally {
      setBusy(false);
    }
  }

  function fill(u: string, p: string) {
    setUsername(u);
    setPassword(p);
  }

  return (
    <div className="auth-wrap">
      <div className="auth-card">
        <div className="auth-logo">
          <div className="circle">校</div>
          <h2>校园移动办公平台</h2>
          <p>校本教学套件 · 数据不出校</p>
        </div>

        <form onSubmit={submit}>
          <div className="field">
            <label>用户名</label>
            <input
              className="input"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="请输入用户名"
              autoComplete="username"
            />
          </div>
          <div className="field">
            <label>密码</label>
            <input
              className="input"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="请输入密码"
              autoComplete="current-password"
            />
          </div>
          {err && <div className="error-text">{err}</div>}
          <button className="btn" disabled={busy} style={{ marginTop: 8 }}>
            {busy ? '登录中…' : '登 录'}
          </button>
        </form>

        <div className="hint" style={{ marginTop: 18 }}>
          演示账号（点击快速填充）：
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 8 }}>
            {DEMO.map((d) => (
              <button
                key={d.username}
                type="button"
                className="btn ghost"
                style={{ width: 'auto', padding: '6px 12px', fontSize: 13 }}
                onClick={() => fill(d.username, d.password)}
              >
                {d.role}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
