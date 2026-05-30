import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api, type CustomPattern } from '../api/client';
import { Badge, Banner, ConfirmDialog, ErrorState, PageHead, Spinner } from '../components/ui';

const EMPTY: CustomPattern = { name: '', regex: '', category: '' };

export function CategoriesPage() {
  const qc = useQueryClient();
  const [editing, setEditing] = useState<CustomPattern | null>(null);
  const [isNew, setIsNew] = useState(false);
  const [form, setForm] = useState<CustomPattern>(EMPTY);
  const [confirmDelete, setConfirmDelete] = useState<CustomPattern | null>(null);

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['categories'],
    queryFn: api.listCategories,
  });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ['categories'] });
    qc.invalidateQueries({ queryKey: ['system'] });
  };

  const save = useMutation({
    mutationFn: () =>
      isNew ? api.createRule(form) : api.updateRule(editing!.name, form),
    onSuccess: () => {
      setEditing(null);
      setIsNew(false);
      invalidate();
    },
  });

  const del = useMutation({
    mutationFn: (name: string) => api.deleteRule(name),
    onSuccess: () => {
      setConfirmDelete(null);
      invalidate();
    },
  });

  if (isLoading) return <Spinner label="Loading categories…" />;
  if (error || !data) return <ErrorState error={error} onRetry={() => refetch()} />;

  let regexError: string | null = null;
  if (form.regex) {
    try {
      new RegExp(form.regex);
    } catch (e) {
      regexError = (e as Error).message;
    }
  }
  const valid = form.name.trim() && form.category.trim() && form.regex && !regexError;

  function startNew() {
    setForm(EMPTY);
    setIsNew(true);
    setEditing(EMPTY);
  }
  function startEdit(rule: CustomPattern) {
    setForm(rule);
    setIsNew(false);
    setEditing(rule);
  }

  return (
    <div>
      <PageHead
        title="Categories & Rules"
        subtitle="Built-in detection families (from the active config) and custom regex rules."
        actions={
          <button className="btn primary" onClick={startNew}>
            + New rule
          </button>
        }
      />

      <div className="grid cols-2" style={{ marginBottom: 20 }}>
        <div className="card">
          <h3>Detection families</h3>
          <div className="grid" style={{ gridTemplateColumns: '1fr 1fr', gap: 8 }}>
            {data.families.map((f) => (
              <div key={f.key} className="row" style={{ justifyContent: 'space-between' }}>
                <span>{f.label}</span>
                {f.enabled ? <Badge tone="ok">on</Badge> : <Badge tone="muted">off</Badge>}
              </div>
            ))}
          </div>
          <p className="muted" style={{ fontSize: 12, marginTop: 12, marginBottom: 0 }}>
            Toggle families by activating a profile or editing the active policy.
          </p>
        </div>
        <div className="card">
          <h3>NER entity types</h3>
          {data.ner_entity_types.length === 0 ? (
            <p className="muted">All default entity types (no explicit filter).</p>
          ) : (
            <div className="row" style={{ flexWrap: 'wrap', gap: 6 }}>
              {data.ner_entity_types.map((t) => (
                <Badge key={t} tone="muted">
                  {t}
                </Badge>
              ))}
            </div>
          )}
        </div>
      </div>

      <h3 className="muted" style={{ textTransform: 'uppercase', fontSize: 12 }}>
        Custom regex rules
      </h3>
      {save.error && <Banner tone="danger">{(save.error as Error).message}</Banner>}

      {data.custom_rules.length === 0 ? (
        <div className="state">
          <h3>No custom rules</h3>
          <p className="muted">Add a regex rule to detect organisation-specific identifiers.</p>
        </div>
      ) : (
        <table className="data">
          <thead>
            <tr>
              <th>Name</th>
              <th>Category</th>
              <th>Pattern</th>
              <th style={{ width: 140 }}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {data.custom_rules.map((r) => (
              <tr key={r.name}>
                <td className="mono">{r.name}</td>
                <td>
                  <Badge tone="muted">{r.category}</Badge>
                </td>
                <td className="mono" style={{ maxWidth: 360, overflow: 'hidden' }}>
                  {r.regex}
                </td>
                <td>
                  <div className="row">
                    <button className="btn sm" onClick={() => startEdit(r)}>
                      Edit
                    </button>
                    <button className="btn sm danger" onClick={() => setConfirmDelete(r)}>
                      Delete
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {editing && (
        <div className="modal-overlay" onClick={() => setEditing(null)}>
          <div
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-label={isNew ? 'New custom rule' : 'Edit custom rule'}
            onClick={(e) => e.stopPropagation()}
          >
            <h2>{isNew ? 'New custom rule' : `Edit "${editing.name}"`}</h2>
            <div className="grid" style={{ gap: 12 }}>
              <label className="field">
                Name
                <input
                  type="text"
                  value={form.name}
                  onChange={(e) => setForm({ ...form, name: e.target.value })}
                />
              </label>
              <label className="field">
                Category
                <input
                  type="text"
                  value={form.category}
                  placeholder="EMPLOYEE_ID"
                  onChange={(e) => setForm({ ...form, category: e.target.value })}
                />
              </label>
              <label className="field">
                Regex pattern
                <input
                  type="text"
                  className="mono"
                  value={form.regex}
                  placeholder="EMP-\\d{4}"
                  onChange={(e) => setForm({ ...form, regex: e.target.value })}
                />
                {regexError && <span className="field-error">Invalid regex: {regexError}</span>}
              </label>
            </div>
            <div className="modal-actions">
              <button className="btn" onClick={() => setEditing(null)}>
                Cancel
              </button>
              <button
                className="btn primary"
                disabled={!valid || save.isPending}
                onClick={() => save.mutate()}
              >
                {save.isPending ? 'Saving…' : 'Save'}
              </button>
            </div>
          </div>
        </div>
      )}

      {confirmDelete && (
        <ConfirmDialog
          title={`Delete rule "${confirmDelete.name}"?`}
          body="This removes the custom regex rule from the live detection config."
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
