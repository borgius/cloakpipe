import { Link, Outlet, useRouterState } from '@tanstack/react-router';
import { BASE_URL } from '../api/client';

const NAV = [
  { to: '/', label: 'Overview', exact: true },
  { to: '/profiles', label: 'Profiles' },
  { to: '/policies', label: 'Policies' },
  { to: '/categories', label: 'Categories & Rules' },
  { to: '/audit', label: 'Audit Logs' },
  { to: '/vault', label: 'Vault & Secrets' },
  { to: '/sessions', label: 'Sessions' },
];

export function RootLayout() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });

  return (
    <div className="app">
      <nav className="sidebar" aria-label="Main navigation">
        <div className="brand">
          🛡️ CloakPipe
          <small>admin</small>
        </div>
        {NAV.map((item) => {
          const active = item.exact ? pathname === item.to : pathname.startsWith(item.to);
          return (
            <Link
              key={item.to}
              to={item.to}
              className={`nav-link ${active ? 'active' : ''}`}
              aria-current={active ? 'page' : undefined}
            >
              {item.label}
            </Link>
          );
        })}
        <div className="spacer" />
        <div className="muted" style={{ fontSize: 11, padding: '8px 12px' }}>
          {BASE_URL ? (
            <>
              API: <span className="mono">{BASE_URL}</span>
            </>
          ) : (
            'API: same-origin'
          )}
        </div>
      </nav>
      <main className="main">
        <Outlet />
      </main>
    </div>
  );
}
