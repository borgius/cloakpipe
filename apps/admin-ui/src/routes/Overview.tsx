import { useQuery } from '@tanstack/react-query';
import { api } from '../api/client';
import { Badge, Banner, ErrorState, PageHead, Spinner } from '../components/ui';

export function OverviewPage() {
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['system'],
    queryFn: api.getSystem,
  });

  if (isLoading) return <Spinner label="Loading system status…" />;
  if (error || !data) return <ErrorState error={error} onRetry={() => refetch()} />;

  const enabledFamilies = Object.entries(data.detection ?? {}).filter(
    ([k, v]) => k !== 'custom_pattern_count' && v === true,
  ).length;

  return (
    <div>
      <PageHead
        title="Overview"
        subtitle="Live runtime and configuration status for this CloakPipe server instance."
      />

      {data.mode !== 'server' && (
        <Banner tone="danger">
          This instance is running in <strong>{data.mode}</strong> mode. The admin API requires{' '}
          <code className="inline">server</code> mode.
        </Banner>
      )}

      <div className="grid cols-4" style={{ marginBottom: 16 }}>
        <div className="card">
          <div className="stat-label">Active profile</div>
          <div className="stat">{data.active_profile ?? '—'}</div>
        </div>
        <div className="card">
          <div className="stat-label">Masking strategy</div>
          <div className="stat" style={{ fontSize: 18 }}>
            {data.masking_strategy}
          </div>
        </div>
        <div className="card">
          <div className="stat-label">Detection families on</div>
          <div className="stat">{enabledFamilies}</div>
        </div>
        <div className="card">
          <div className="stat-label">Vault mappings</div>
          <div className="stat">{data.vault?.total_mappings ?? 0}</div>
        </div>
      </div>

      <div className="grid cols-2">
        <div className="card">
          <h3>Runtime</h3>
          <dl className="kv">
            <dt>Service</dt>
            <dd>
              {data.service} v{data.version}
            </dd>
            <dt>Mode</dt>
            <dd>
              <Badge tone={data.mode === 'server' ? 'ok' : 'warn'}>{data.mode}</Badge>
            </dd>
            <dt>Listen</dt>
            <dd>{data.listen}</dd>
            <dt>Upstream</dt>
            <dd>{data.upstream}</dd>
            <dt>Config path</dt>
            <dd>{data.config_path ?? '—'}</dd>
            <dt>Policies dir</dt>
            <dd>{data.policies_dir ?? '—'}</dd>
            <dt>Profiles dir</dt>
            <dd>{data.profiles_dir ?? '—'}</dd>
            <dt>Admin auth</dt>
            <dd>
              {data.auth_required ? (
                <Badge tone="ok">token required</Badge>
              ) : (
                <Badge tone="warn">open</Badge>
              )}
            </dd>
          </dl>
        </div>

        <div className="card">
          <h3>NER</h3>
          <dl className="kv">
            <dt>Status</dt>
            <dd>
              {data.ner?.enabled ? (
                <Badge tone="ok">enabled</Badge>
              ) : (
                <Badge tone="muted">disabled</Badge>
              )}
            </dd>
            <dt>Backend</dt>
            <dd>{data.ner?.backend}</dd>
            <dt>Model</dt>
            <dd>{data.ner?.model ?? 'default'}</dd>
            <dt>Confidence</dt>
            <dd>{data.ner?.confidence_threshold}</dd>
            <dt>Sidecar</dt>
            <dd>{data.ner?.sidecar_url}</dd>
          </dl>
        </div>

        <div className="card">
          <h3>Audit backend</h3>
          <dl className="kv">
            <dt>Status</dt>
            <dd>
              {data.audit?.enabled ? (
                <Badge tone="ok">enabled</Badge>
              ) : (
                <Badge tone="muted">disabled</Badge>
              )}
            </dd>
            <dt>Backend</dt>
            <dd>{data.audit?.backend}</dd>
            <dt>Location</dt>
            <dd>{data.audit?.location ?? '—'}</dd>
            <dt>Retention</dt>
            <dd>{data.audit?.retention_days} days</dd>
          </dl>
        </div>

        <div className="card">
          <h3>Vault backend</h3>
          <dl className="kv">
            <dt>Backend</dt>
            <dd>{data.vault?.backend}</dd>
            <dt>Persistent</dt>
            <dd>
              {data.vault?.persistent ? (
                <Badge tone="ok">yes</Badge>
              ) : (
                <Badge tone="warn">ephemeral</Badge>
              )}
            </dd>
            <dt>Path</dt>
            <dd>{data.vault?.path ?? '—'}</dd>
            <dt>Encryption</dt>
            <dd>{data.vault?.encryption}</dd>
          </dl>
        </div>
      </div>
    </div>
  );
}
