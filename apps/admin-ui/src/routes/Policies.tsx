import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api, type PolicySummary, type ValidationReport } from '../api/client';
import { Badge, Banner, ConfirmDialog, ErrorState, PageHead, Spinner } from '../components/ui';

const TEMPLATE = `# New CloakPipe policy (cloakpipe.toml format)
profile = "general"

[proxy]
listen = "127.0.0.1:8400"
upstream = "https://api.openai.com"
api_key_env = "OPENAI_API_KEY"
mode = "server"
masking_strategy = "token"

[vault]
path = "./vault.enc"
key_env = "CLOAKPIPE_VAULT_KEY"

[detection]
secrets = true
financial = true
emails = true
`;

export function PoliciesPage() {
  const qc = useQueryClient();
  const [selected, setSelected] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState('');
  const [content, setContent] = useState('');
  const [original, setOriginal] = useState('');
  const [validation, setValidation] = useState<ValidationReport | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<PolicySummary | null>(null);
  const [confirmActivate, setConfirmActivate] = useState<string | null>(null);
  const [activateNote, setActivateNote] = useState<string | null>(null);

  const list = useQuery({ queryKey: ['policies'], queryFn: api.listPolicies });

  const dirty = content !== original;

  useEffect(() => {
    const handler = (e: BeforeUnloadEvent) => {
      if (dirty) {
        e.preventDefault();
        e.returnValue = '';
      }
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, [dirty]);

  async function openPolicy(name: string) {
    if (dirty && !window.confirm('Discard unsaved changes?')) return;
    setCreating(false);
    const detail = await api.getPolicy(name);
    setSelected(name);
    setContent(detail.content);
    setOriginal(detail.content);
    setValidation(detail.validation);
  }

  function startCreate() {
    if (dirty && !window.confirm('Discard unsaved changes?')) return;
    setCreating(true);
    setSelected(null);
    setNewName('');
    setContent(TEMPLATE);
    setOriginal('');
    setValidation(null);
  }

  const validate = useMutation({
    mutationFn: () => api.validatePolicy(content),
    onSuccess: setValidation,
  });

  const save = useMutation({
    mutationFn: () => {
      const name = creating ? newName.trim() : selected!;
      return api.putPolicy(name, content);
    },
    onSuccess: (detail) => {
      setOriginal(detail.content);
      setValidation(detail.validation);
      setSelected(detail.name);
      setCreating(false);
      qc.invalidateQueries({ queryKey: ['policies'] });
    },
  });

  const del = useMutation({
    mutationFn: (name: string) => api.deletePolicy(name),
    onSuccess: () => {
      setConfirmDelete(null);
      if (selected === confirmDelete?.name) {
        setSelected(null);
        setContent('');
        setOriginal('');
      }
      qc.invalidateQueries({ queryKey: ['policies'] });
    },
  });

  const activate = useMutation({
    mutationFn: (name: string) => api.activatePolicy(name),
    onSuccess: (res) => {
      setConfirmActivate(null);
      setActivateNote(
        `${res.note}${res.restart_required ? ' (restart required for some changes)' : ''}` +
          (res.warnings && res.warnings.length ? ` — ${res.warnings.join('; ')}` : ''),
      );
      qc.invalidateQueries({ queryKey: ['policies'] });
      qc.invalidateQueries({ queryKey: ['system'] });
    },
  });

  if (list.isLoading) return <Spinner label="Loading policies…" />;
  if (list.error || !list.data)
    return <ErrorState error={list.error} onRetry={() => list.refetch()} />;

  const editing = creating || selected !== null;
  const canSave =
    (creating ? newName.trim().length > 0 : true) && dirty && !save.isPending;

  return (
    <div>
      <PageHead
        title="Policies"
        subtitle="Disk-backed cloakpipe.toml configs. Validated before saving; activation applies detection live."
        actions={
          <button className="btn primary" onClick={startCreate}>
            + New policy
          </button>
        }
      />

      {activateNote && <Banner tone="info">{activateNote}</Banner>}

      <div className="grid" style={{ gridTemplateColumns: '280px 1fr' }}>
        <div className="card" style={{ padding: 8 }}>
          {list.data.length === 0 && <p className="muted" style={{ padding: 12 }}>No policy files.</p>}
          {list.data.map((p) => (
            <div
              key={p.name}
              className={`nav-link ${selected === p.name ? 'active' : ''}`}
              style={{ justifyContent: 'space-between', cursor: 'pointer' }}
              onClick={() => openPolicy(p.name)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => e.key === 'Enter' && openPolicy(p.name)}
            >
              <span className="mono">{p.name}</span>
              {p.active && <Badge tone="ok">active</Badge>}
            </div>
          ))}
        </div>

        <div>
          {!editing ? (
            <div className="card">
              <p className="muted">Select a policy to view or edit, or create a new one.</p>
            </div>
          ) : (
            <div className="card">
              <div className="toolbar">
                {creating ? (
                  <label className="field" style={{ flex: 1 }}>
                    Policy name
                    <input
                      type="text"
                      value={newName}
                      placeholder="my-policy"
                      onChange={(e) => setNewName(e.target.value)}
                      aria-label="Policy name"
                    />
                  </label>
                ) : (
                  <strong className="mono">{selected}</strong>
                )}
                {dirty && <Badge tone="warn">unsaved</Badge>}
              </div>

              <textarea
                className="code"
                value={content}
                spellCheck={false}
                aria-label="Policy content"
                onChange={(e) => setContent(e.target.value)}
              />

              {validation && (
                <div style={{ marginTop: 10 }}>
                  {validation.valid ? (
                    <Banner tone="info">
                      Valid · profile: {validation.profile ?? '—'} · mode: {validation.mode ?? '—'}
                    </Banner>
                  ) : (
                    <Banner tone="danger">
                      Invalid:
                      <ul style={{ margin: '6px 0 0 18px' }}>
                        {validation.errors?.map((er, i) => <li key={i}>{er}</li>)}
                      </ul>
                    </Banner>
                  )}
                </div>
              )}
              {save.error && <Banner tone="danger">{(save.error as Error).message}</Banner>}

              <div className="toolbar" style={{ marginTop: 12, marginBottom: 0 }}>
                <button
                  className="btn"
                  onClick={() => validate.mutate()}
                  disabled={validate.isPending}
                >
                  Validate
                </button>
                <button className="btn primary" onClick={() => save.mutate()} disabled={!canSave}>
                  {save.isPending ? 'Saving…' : 'Save'}
                </button>
                <div className="spacer" />
                {!creating && selected && (
                  <>
                    <button
                      className="btn"
                      onClick={() => setConfirmActivate(selected)}
                      disabled={dirty}
                      title={dirty ? 'Save changes first' : undefined}
                    >
                      Activate
                    </button>
                    <button
                      className="btn danger"
                      onClick={() =>
                        setConfirmDelete(list.data.find((p) => p.name === selected) ?? null)
                      }
                    >
                      Delete
                    </button>
                  </>
                )}
              </div>
            </div>
          )}
        </div>
      </div>

      {confirmDelete && (
        <ConfirmDialog
          title={`Delete policy "${confirmDelete.name}"?`}
          body="This permanently removes the policy file from disk. This cannot be undone."
          confirmLabel="Delete"
          danger
          busy={del.isPending}
          onConfirm={() => del.mutate(confirmDelete.name)}
          onCancel={() => setConfirmDelete(null)}
        />
      )}
      {confirmActivate && (
        <ConfirmDialog
          title={`Activate policy "${confirmActivate}"?`}
          body="This applies the policy's detection settings to the running instance. Listener/upstream/masking changes require a restart."
          confirmLabel="Activate"
          busy={activate.isPending}
          onConfirm={() => activate.mutate(confirmActivate)}
          onCancel={() => setConfirmActivate(null)}
        />
      )}
    </div>
  );
}
