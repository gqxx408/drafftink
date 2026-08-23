import { type ReactNode } from 'react';
import { HashRouter, Navigate, Route, Routes } from 'react-router-dom';
import { useAuth } from './auth/AuthContext';
import Login from './pages/Login';
import Mfa from './pages/Mfa';
import Home from './pages/Home';
import Approvals from './pages/Approvals';
import OfficialDoc from './pages/OfficialDoc';
import Announcements from './pages/Announcements';
import Meeting from './pages/Meeting';
import Seal from './pages/Seal';
import Messages from './pages/Messages';
import AIDashboard from './pages/AIDashboard';
import Me from './pages/Me';

function RequireAuth({ children }: { children: ReactNode }) {
  const { stage, loading } = useAuth();
  if (loading) {
    return (
      <div className="auth-wrap">
        <div className="auth-card center">加载中…</div>
      </div>
    );
  }
  if (stage === 'anonymous') return <Navigate to="/login" replace />;
  if (stage === 'mfa') return <Navigate to="/mfa" replace />;
  return <>{children}</>;
}

function MfaGate() {
  const { stage, loading } = useAuth();
  if (loading) return <div className="auth-wrap"><div className="auth-card center">加载中…</div></div>;
  if (stage === 'anonymous') return <Navigate to="/login" replace />;
  if (stage === 'authed') return <Navigate to="/" replace />;
  return <Mfa />;
}

export default function App() {
  return (
    <HashRouter>
      <Routes>
        <Route path="/login" element={<Login />} />
        <Route path="/mfa" element={<MfaGate />} />
        <Route
          path="/"
          element={
            <RequireAuth>
              <Home />
            </RequireAuth>
          }
        />
        <Route
          path="/approvals"
          element={
            <RequireAuth>
              <Approvals />
            </RequireAuth>
          }
        />
        <Route
          path="/official-doc"
          element={
            <RequireAuth>
              <OfficialDoc />
            </RequireAuth>
          }
        />
        <Route
          path="/announcements"
          element={
            <RequireAuth>
              <Announcements />
            </RequireAuth>
          }
        />
        <Route
          path="/meeting"
          element={
            <RequireAuth>
              <Meeting />
            </RequireAuth>
          }
        />
        <Route
          path="/seal"
          element={
            <RequireAuth>
              <Seal />
            </RequireAuth>
          }
        />
        <Route
          path="/messages"
          element={
            <RequireAuth>
              <Messages />
            </RequireAuth>
          }
        />
        <Route
          path="/ai"
          element={
            <RequireAuth>
              <AIDashboard />
            </RequireAuth>
          }
        />
        <Route
          path="/me"
          element={
            <RequireAuth>
              <Me />
            </RequireAuth>
          }
        />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </HashRouter>
  );
}
