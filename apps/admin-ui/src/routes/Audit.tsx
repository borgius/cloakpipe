import { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { ColumnDef } from '@tanstack/react-table';
import { api, type AuditEntry } from '../api/client';
import { DataTable } from '../components/DataTable';
import { Badge, Banner, ErrorState, PageHead, Spinner } from '../components/ui';

interface Filters {
  event: string;
  surface: string;
  session_id: string;
  since: string;
  until: string;
}

const EMPTY: Filters = { event: '', surface: '', session_id: '', since: '', until: '' };

export function AuditPage() {
  const [filters, setFilters] = useState<Filters>(EMPTY);
  const [applied, setApplied] = useState<Filters>(EMPTY);

  const events = useQuery({
    queryKey: ['audit', applied],
    queryFn: () =>
      api.queryAudit({
        event: applied.event || undefined,
        surface: applied.surface || undefined,
        session_id: applied.session_id || undefined,
        since: applied.since || undefined,
        until: applied.until || undefined,
        limit: 500,
      }),
  });

  const summary = useQuery({ queryKey: ['audit-summary'], queryFn: api.auditSummary });

  const columns = useMemo<ColumnDef<AuditEntry, unknown>[]>(
    () => [
      { accessorKey: 'timestamp', header: 'Time' },
      {
        accessorKey: 'event',
        header: 'Event',
        cell: (c) => <Badge tone="muted">{String(c.getValue())}</Badge>,
      },
      { accessorKey: 'surface', header: 'Surface' },
      {
        accessorKey: 'session_id',
        header: 'Session',
        cell: (c) => <span className="mono">{(c.getValue() as string) ?? '—'}</span>,
      },
      { accessorKey: 'entities_detected', header: 'Detected' },
      { accessorKey: 'entities_replaced', header: 'Replaced' },
    ],
    [],
  );

  return (
    <div>
      <PageHead
        title="Audit Logs"
        subtitle="Query privacy events recorded by the audit backend."
        actions={
          events.data?.supported ? (
            <a className="btn" href={api.auditExportUrl()}>
              Export CSV
            </a>
          ) : null
        }
      />

      {summary.data && summary.data.supported && (
        <div className="grid cols-4" style={{ marginBottom: 16 }}>
          <div className="card">
            <div className="stat-label">Total events</div>
            <div className="stat">{summary.data.total}</div>
          </div>
          {summary.data.counts.slice(0, 3).map((c) => (
            <div className="card" key={c.event}>
              <div className="stat-label">{c.event}</div>
              <div className="stat">{c.count}</div>
            </div>
          ))}
        </div>
      )}

      <div className="card" style={{ marginBottom: 16 }}>
        <div className="toolbar" style={{ marginBottom: 0 }}>
          <input
            type="text"
            placeholder="event"
            aria-label="Filter by event"
            value={filters.event}
            onChange={(e) => setFilters({ ...filters, event: e.target.value })}
          />
          <input
            type="text"
            placeholder="surface"
            aria-label="Filter by surface"
            value={filters.surface}
            onChange={(e) => setFilters({ ...filters, surface: e.target.value })}
          />
          <input
            type="text"
            placeholder="session id"
            aria-label="Filter by session id"
            value={filters.session_id}
            onChange={(e) => setFilters({ ...filters, session_id: e.target.value })}
          />
          <input
            type="text"
            placeholder="since (RFC3339)"
            aria-label="Filter since"
            value={filters.since}
            onChange={(e) => setFilters({ ...filters, since: e.target.value })}
          />
          <button className="btn primary" onClick={() => setApplied(filters)}>
            Apply
          </button>
          <button
            className="btn"
            onClick={() => {
              setFilters(EMPTY);
              setApplied(EMPTY);
            }}
          >
            Reset
          </button>
        </div>
      </div>

      {events.isLoading ? (
        <Spinner label="Querying audit events…" />
      ) : events.error ? (
        <ErrorState error={events.error} onRetry={() => events.refetch()} />
      ) : !events.data?.supported ? (
        <Banner tone="warn">
          The audit backend (<strong>{events.data?.backend ?? 'unknown'}</strong>) does not support
          querying over HTTP. SQLite is fully supported; JSONL is read on a best-effort basis. Enable
          a supported audit backend in your policy to query here.
        </Banner>
      ) : (
        <>
          <p className="muted" style={{ fontSize: 12 }}>
            Backend: <code className="inline">{events.data.backend}</code> · {events.data.total_matched}{' '}
            matched
          </p>
          <DataTable
            data={events.data.events}
            columns={columns}
            emptyTitle="No matching audit events"
            emptyBody="Adjust filters or generate some traffic through the proxy."
            pageSize={50}
          />
        </>
      )}
    </div>
  );
}
