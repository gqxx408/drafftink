import { NavLink } from 'react-router-dom';

const items = [
  { to: '/', label: '首页', icon: '🏠', end: true },
  { to: '/approvals', label: '审批', icon: '✅' },
  { to: '/official-doc', label: '公文', icon: '📄' },
  { to: '/announcements', label: '公告', icon: '📢' },
  { to: '/me', label: '我的', icon: '👤' },
];

export default function BottomNav() {
  return (
    <nav className="bottom-nav">
      {items.map((it) => (
        <NavLink
          key={it.to}
          to={it.to}
          end={it.end}
          className={({ isActive }) => (isActive ? 'active' : '')}
        >
          <span className="nav-icon">{it.icon}</span>
          <span>{it.label}</span>
        </NavLink>
      ))}
    </nav>
  );
}
