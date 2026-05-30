import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { api } from '../api/client';
import { Badge, Banner, ConfirmDialog, ErrorState, PageHead, Spinner } from '../components/ui';

export function VaultPage() {
  const [reveal, setReveal] = useState(false);
  const [confirmReveal, setConfirmReveal] = useState(false);
  const [search, setSearch] = useState('');
  const [category, setCategory] = useState('');

  const stats = useQuery({ queryKey: ['vault-stats'], queryFn: api.vaultStats });
  const mappings = useQuery({
    queryKey: ['vault-mappings', reveal, search, category],
    queryFn: () =>
      api.vaultMappings({
        reveal: reveal || undefined,
        search: search || undefined,
        category: category || undefined,
        limit: 200,
      }),
  });

  return (
    <div>
      <PageHead
        title="Vault & Secrets"
        subtitle="Inspect token↔original mappings. Originals are sensitive and redacted by default."
      />

      <Banner tone="danger">
        ⚠️ Revealing original values exposes the very PII CloakPipe is protecting and writes an audit
        event. Only do this on a trusted, local machine.
      </Banner>

      {stats.isLoading ? (
        <Spinner />
      ) : stats.error || !stats.data ? (
        <ErrorState error={stats.error} onRetry={() => stats.refetch()} />
      ) : (
        <div className="grid cols-4" style={{ marginBottom: 16 }}>
          <div className="card">
            <div className="stat-label">Total mappings</div>
            <div className="stat">{stats.data.total_mappings}</div>
          </div>
          <div className="card">
            <div className="stat-label">Backend</div>
            <div className="stat" style={{ fontSize: 18 }}>
              {stats.data.backend}
            </div>
          </div>
          <div className="card">
            <div className="stat-label">Persistent</div>
            <div className="stat" style={{ fontSize: 18 }}>
              {stats.data.persistent ? 'yes' : 'ephemeral'}
            </div>
          </div>
          <div className="card">
            <div className="stat-label">Categories</div>
            <div className="stat">{Object.keys(stats.data.categories ?? {}).length}</div>
          </div>
        </div>
      )}

      <div className="card" style={{ marginBottom: 16 }}>
        <div className="toolbar" style={{ marginBottom: 0 }}>
          <input
            type="search"
            placeholder={reveal ? 'search token or original' : 'search token'}
            aria-label="Search mappings"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <input
            type="text"
            placeholder="category"
            aria-label="Filter by category"
            value={category}
            onChange={(e) => setCategory(e.target.value)}
          />
          <div className="spacer" />
          {reveal ? (
            <button className="btn" onClick={() => setReveal(false)}>
              Hide originals
            </button>
          ) : (
            <button className="btn danger" onClick={() => setConfirmReveal(true)}>
              Reveal originals
            </button>
          )}
        </div>
      </div>

      {mappings.data?.warning && <Banner tone="warn">{mappings.data.warning}</Banner>}

      {mappings.isLoading ? (
        <Spinner label="Loading mappings…" />
      ) : mappings.error ? (
        <ErrorState error={mappings.error} onRetry={() => mappings.refetch()} />
      ) : mappings.data && mappings.data.mappings.length === 0 ? (
        <div className="state">
          <h3>No mappings</h3>
          <p className="muted">The vault is empty or no rows matched your filter.</p>
        </div>
      ) : (
        <table className="data">
          <thead>
            <tr>
              <th>Token</th>
              <th>Category</th>
              <th>Original</th>
            </tr>
          </thead>
          <tbody>
            {mappings.data?.mappings.map((m, i) => (
              <tr key={`${m.token}-${i}`}>
                <td className="mono">{m.token}</td>
                <td>
                  <Badge tone="muted">{m.category}</Badge>
                </td>
                <td className="mono">
                  {m.redacted ? <span className="muted">{m.original}</span> : m.original}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {confirmReveal && (
        <ConfirmDialog
          title="Reveal original values?"
          body="This decrypts and displays the original PII for all listed tokens and records an audit event. Continue only on a trusted machine."
          confirmLabel="Reveal"
          danger
          onConfirm={() => {
            setReveal(true);
            setConfirmReveal(false);
          }}
          onCancel={() => setConfirmReveal(false)}
        />
      )}
    </div>
  );
}
