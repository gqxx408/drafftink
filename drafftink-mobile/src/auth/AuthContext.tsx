import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import {
  clearSession,
  getAccessToken,
  getStoredUser,
  login as apiLogin,
  mfaVerify as apiMfaVerify,
  saveSession,
} from '../api/client';
import type { LoginResponse, UserInfo } from '../api/types';

const AUTHED_KEY = 'drafftink_authed';

interface AuthState {
  user: UserInfo | null;
  // 'anonymous'：未登录；'mfa'：已登录待短信二次验证；'authed'：已完成 MFA
  stage: 'anonymous' | 'mfa' | 'authed';
  loading: boolean;
}

interface AuthContextValue extends AuthState {
  login: (username: string, password: string) => Promise<LoginResponse>;
  verifyMfa: (code: string) => Promise<void>;
  logout: () => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<UserInfo | null>(null);
  const [stage, setStage] = useState<AuthState['stage']>('anonymous');
  const [loading, setLoading] = useState(true);

  // 启动时根据持久化的令牌恢复登录态
  useEffect(() => {
    const token = getAccessToken();
    const stored = getStoredUser();
    if (token && stored) {
      setUser(stored);
      setStage(localStorage.getItem(AUTHED_KEY) === '1' ? 'authed' : 'mfa');
    }
    setLoading(false);
  }, []);

  const login = useCallback(async (username: string, password: string) => {
    const resp = await apiLogin(username, password);
    saveSession(resp);
    setUser(resp.user);
    setStage('mfa'); // 进入短信二次验证环节
    return resp;
  }, []);

  const verifyMfa = useCallback(async (code: string) => {
    await apiMfaVerify(getAccessToken()!, code);
    localStorage.setItem(AUTHED_KEY, '1');
    setStage('authed');
  }, []);

  const logout = useCallback(() => {
    clearSession();
    localStorage.removeItem(AUTHED_KEY);
    setUser(null);
    setStage('anonymous');
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({ user, stage, loading, login, verifyMfa, logout }),
    [user, stage, loading, login, verifyMfa, logout],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth 必须在 AuthProvider 内使用');
  return ctx;
}
