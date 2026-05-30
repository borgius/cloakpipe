import { useState } from 'react';
import { Link, Outlet, useRouterState } from '@tanstack/react-router';
import { BASE_URL, getAdminToken, setAdminToken } from '../api/client';

const NAV = [
  { to: '/', label: 'Overview', exact: true },
  { to: '/profiles', label: 'Profiles' },
  { to: '/policies', label: 'Policies' },
  { to: '/categories', label: 'Categories & Rules' },
  { to: '/audit', label: 'Audit Logs' },
  { to: '/vault', label: 'Vault & Secrets' },
  { to: '/sessions', label: 'Sessions' },
];

function AdminTokenControl() {
  const [token, setToken] = useState(getAdminToken() ?? '');
  const [saved, setSaved] = useState(false);

  function apply() {
    setAdminToken(token);
    setSaved(true);
    window.setTimeout(() => setSaved(false), 1500);
  }

  return (
    <details className="token-control">
      <summary className="muted" style={{ fontSize: 11, padding: '8px 12px', cursor: 'pointer' }}>
        Admin token {getAdminToken() ? '✓' : ''}
      </summary>
      <div style={{ padding: '4px 12px 8px' }}>
        <input
          type="password"
          value={token}
          placeholder="******"
          aria-label="Admin token"
          onChange={(e) => setToken(e.target.value)}
          style={{ width: '100%', fontSize: 12 }}
        />
        <div className="row" style={{ marginTop: 6 }}>
          <button className="btn sm" onClick={apply}>
            {saved ? 'Saved' : 'Save'}
          </button>
          {getAdminToken() && (
            <button
              className="btn sm"
              onClick={() => {
                setAdminToken(null);
                setToken('');
              }}
            >
              Clear
            </button>
          )}
        </div>
      </div>
    </details>
  );
}

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
        <AdminTokenControl />
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
