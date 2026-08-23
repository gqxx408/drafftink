import type { ReactNode } from 'react';
import { useAuth } from '../auth/AuthContext';
import BottomNav from './BottomNav';

interface LayoutProps {
  title: string;
  subtitle?: string;
  children: ReactNode;
}

export default function Layout({ title, subtitle, children }: LayoutProps) {
  const { user } = useAuth();
  const initial = (user?.display_name || user?.username || '?').slice(0, 1);
  return (
    <div className="app">
      <header className="app-header">
        <div>
          <h1>{title}</h1>
          {subtitle && <div className="sub">{subtitle}</div>}
        </div>
        {user && (
          <div className="header-user">
            <span>{user.display_name}</span>
            <div className="avatar">{initial}</div>
          </div>
        )}
      </header>
      <main className="app-main">{children}</main>
      <BottomNav />
    </div>
  );
}
