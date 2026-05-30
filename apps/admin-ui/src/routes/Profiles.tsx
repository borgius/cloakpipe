import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api, type ProfileSummary } from '../api/client';
import { Badge, Banner, ConfirmDialog, ErrorState, PageHead, Spinner } from '../components/ui';

interface NewProfileForm {
  name: string;
  description: string;
  base: string;
}

export function ProfilesPage() {
  const qc = useQueryClient();
  const [pending, setPending] = useState<ProfileSummary | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<ProfileSummary | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [creating, setCreating] = useState(false);
  const [form, setForm] = useState<NewProfileForm>({ name: '', description: '', base: '' });

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['profiles'],
    queryFn: api.listProfiles,
  });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ['profiles'] });
    qc.invalidateQueries({ queryKey: ['system'] });
    qc.invalidateQueries({ queryKey: ['categories'] });
  };

  const activate = useMutation({
    mutationFn: (name: string) => api.activateProfile(name),
    onSuccess: (detail) => {
      setWarnings(detail.warnings ?? []);
      setPending(null);
      invalidate();
    },
  });

  const create = useMutation({
    mutationFn: async () => {
      const base = await api.getProfile(form.base);
      return api.createProfile({
        name: form.name.trim(),
        description: form.description.trim(),
        detection: base.detection,
      });
    },
    onSuccess: () => {
      setCreating(false);
      setForm({ name: '', description: '', base: '' });
      invalidate();
    },
  });

  const del = useMutation({
    mutationFn: (name: string) => api.deleteProfile(name),
    onSuccess: () => {
      setConfirmDelete(null);
      invalidate();
    },
  });

  if (isLoading) return <Spinner label="Loading profiles…" />;
  if (error || !data) return <ErrorState error={error} onRetry={() => refetch()} />;

  const nameTaken = data.some((p) => p.name.toLowerCase() === form.name.trim().toLowerCase());
  const nameValid = /^[a-z0-9][a-z0-9_-]*$/i.test(form.name.trim());
  const createValid = nameValid && !nameTaken && form.base;

  function startNew() {
    setForm({ name: '', description: '', base: data && data.length > 0 ? data[0].name : '' });
    setCreating(true);
  }

  return (
    <div>
      <PageHead
        title="Profiles"
        subtitle="Built-in industry templates and your own custom profiles. Activating a profile applies its detection settings live."
        actions={
          <button className="btn primary" onClick={startNew}>
            + New profile
          </button>
        }
      />

      {warnings.length > 0 && (
        <Banner tone="warn">
          {warnings.map((w, i) => (
            <div key={i}>{w}</div>
          ))}
        </Banner>
      )}
      {activate.error && <Banner tone="danger">{(activate.error as Error).message}</Banner>}
      {del.error && <Banner tone="danger">{(del.error as Error).message}</Banner>}

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
            <div className="row">
              <button
                className="btn primary sm"
                disabled={p.active || activate.isPending}
                onClick={() => setPending(p)}
              >
                {p.active ? 'Active' : 'Activate'}
              </button>
              {p.kind === 'custom' && (
                <button
                  className="btn sm danger"
                  disabled={p.active || del.isPending}
                  onClick={() => setConfirmDelete(p)}
                  title={p.active ? 'Cannot delete the active profile' : 'Delete custom profile'}
                >
                  Delete
                </button>
              )}
            </div>
          </div>
        ))}
      </div>

      {creating && (
        <div className="modal-overlay" onClick={() => setCreating(false)}>
          <div
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-label="New custom profile"
            onClick={(e) => e.stopPropagation()}
          >
            <h2>New custom profile</h2>
            {create.error && <Banner tone="danger">{(create.error as Error).message}</Banner>}
            <div className="grid" style={{ gap: 12 }}>
              <label className="field">
                Name
                <input
                  type="text"
                  value={form.name}
                  placeholder="my-profile"
                  onChange={(e) => setForm({ ...form, name: e.target.value })}
                />
                {form.name.trim() !== '' && !nameValid && (
                  <span className="field-error">
                    Use letters, digits, dashes or underscores (must start alphanumeric).
                  </span>
                )}
                {nameTaken && <span className="field-error">A profile with this name exists.</span>}
              </label>
              <label className="field">
                Description
                <input
                  type="text"
                  value={form.description}
                  placeholder="Optional summary"
                  onChange={(e) => setForm({ ...form, description: e.target.value })}
                />
              </label>
              <label className="field">
                Copy detection settings from
                <select
                  value={form.base}
                  onChange={(e) => setForm({ ...form, base: e.target.value })}
                >
                  {data.map((p) => (
                    <option key={p.name} value={p.name}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <div className="modal-actions">
              <button className="btn" onClick={() => setCreating(false)}>
                Cancel
              </button>
              <button
                className="btn primary"
                disabled={!createValid || create.isPending}
                onClick={() => create.mutate()}
              >
                {create.isPending ? 'Creating…' : 'Create'}
              </button>
            </div>
          </div>
        </div>
      )}

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

      {confirmDelete && (
        <ConfirmDialog
          title={`Delete profile "${confirmDelete.name}"?`}
          body="This permanently removes the custom profile definition from disk."
          confirmLabel="Delete"
          danger
          busy={del.isPending}
          onConfirm={() => del.mutate(confirmDelete.name)}
          onCancel={() => setConfirmDelete(null)}
        />
      )}
    </div>
  );
}
