import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api, type ProfileSummary } from '../api/client';
import { Badge, Banner, ConfirmDialog, ErrorState, PageHead, Spinner } from '../components/ui';

export function ProfilesPage() {
  const qc = useQueryClient();
  const [pending, setPending] = useState<ProfileSummary | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['profiles'],
    queryFn: api.listProfiles,
  });

  const activate = useMutation({
    mutationFn: (name: string) => api.activateProfile(name),
    onSuccess: (detail) => {
      setWarnings(detail.warnings ?? []);
      setPending(null);
      qc.invalidateQueries({ queryKey: ['profiles'] });
      qc.invalidateQueries({ queryKey: ['system'] });
      qc.invalidateQueries({ queryKey: ['categories'] });
    },
  });

  if (isLoading) return <Spinner label="Loading profiles…" />;
  if (error || !data) return <ErrorState error={error} onRetry={() => refetch()} />;

  return (
    <div>
      <PageHead
        title="Profiles"
        subtitle="Built-in industry templates. Activating a profile applies its detection settings live."
      />

      {warnings.length > 0 && (
        <Banner tone="warn">
          {warnings.map((w, i) => (
            <div key={i}>{w}</div>
          ))}
        </Banner>
      )}
      {activate.error && <Banner tone="danger">{(activate.error as Error).message}</Banner>}

      <div className="grid cols-3">
        {data.map((p) => (
          <div className="card" key={p.name}>
            <div className="row" style={{ justifyContent: 'space-between' }}>
              <strong style={{ textTransform: 'capitalize' }}>{p.name}</strong>
              {p.active ? <Badge tone="ok">active</Badge> : <Badge tone="muted">{p.kind}</Badge>}
            </div>
            <p className="muted" style={{ minHeight: 54, marginTop: 8 }}>
              {p.description}
            </p>
            <button
              className="btn primary sm"
              disabled={p.active || activate.isPending}
              onClick={() => setPending(p)}
            >
              {p.active ? 'Active' : 'Activate'}
            </button>
          </div>
        ))}
      </div>

      {pending && (
        <ConfirmDialog
          title={`Activate "${pending.name}" profile?`}
          body={
            <>
              This replaces the live detection configuration with the{' '}
              <strong>{pending.name}</strong> template. Existing custom rules will be replaced by the
              profile defaults.
            </>
          }
          confirmLabel="Activate"
          busy={activate.isPending}
          onConfirm={() => activate.mutate(pending.name)}
          onCancel={() => setPending(null)}
        />
      )}
    </div>
  );
}
