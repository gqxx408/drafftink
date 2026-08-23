import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '../auth/AuthContext';
import { ApiRequestError, fetchDevSmsCode, getAccessToken } from '../api/client';
import { useToast } from '../components/Toast';

export default function Mfa() {
  const { verifyMfa } = useAuth();
  const nav = useNavigate();
  const toast = useToast();
  const [code, setCode] = useState('');
  const [busy, setBusy] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [err, setErr] = useState('');

  async function fetchDemoCode() {
    const token = getAccessToken();
    if (!token) {
      setErr('会话已失效，请重新登录');
      return;
    }
    setFetching(true);
    try {
      const r = await fetchDevSmsCode(token);
      if (r.code) {
        setCode(r.code);
        toast('已填入演示验证码');
      } else {
        setErr('未获取到演示验证码（请确认后端已开启 DRAFTTINK_DEV_MODE）');
      }
    } catch (e) {
      setErr(e instanceof ApiRequestError ? e.message : '获取演示验证码失败');
    } finally {
      setFetching(false);
    }
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErr('');
    try {
      await verifyMfa(code.trim());
      toast('二次验证通过，欢迎');
      nav('/');
    } catch (e) {
      setErr(e instanceof ApiRequestError ? e.message : '验证失败');
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="auth-wrap">
      <div className="auth-card">
        <div className="auth-logo">
          <div className="circle">🔐</div>
          <h2>短信二次验证</h2>
          <p>请输入手机收到的 6 位验证码</p>
        </div>

        <form onSubmit={submit}>
          <div className="field">
            <label>短信验证码</label>
            <input
              className="input"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder="6 位验证码"
              inputMode="numeric"
              maxLength={6}
            />
          </div>
          {err && <div className="error-text">{err}</div>}
          <button className="btn" disabled={busy || code.length < 4} style={{ marginTop: 8 }}>
            {busy ? '验证中…' : '完 成 验 证'}
          </button>
        </form>

        <div className="hint" style={{ marginTop: 16 }}>
          没有真实短信通道？点击获取演示验证码（需后端 <code>DRAFTTINK_DEV_MODE=true</code>）：
          <div style={{ marginTop: 8 }}>
            <button
              type="button"
              className="btn ghost"
              style={{ width: 'auto', padding: '8px 14px', fontSize: 13 }}
              onClick={fetchDemoCode}
              disabled={fetching}
            >
              {fetching ? '获取中…' : '获取演示验证码'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
