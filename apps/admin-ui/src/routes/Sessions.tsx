import { useQuery } from '@tanstack/react-query';
import { api } from '../api/client';
import { ErrorState, PageHead, Spinner } from '../components/ui';

export function SessionsPage() {
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['sessions'],
    queryFn: api.listSessions,
  });

  return (
    <div>
      <PageHead
        title="Sessions"
        subtitle="Runtime session diagnostics for the coreference / session engine."
        actions={
          <button className="btn" onClick={() => refetch()}>
            Refresh
          </button>
        }
      />
      {isLoading ? (
        <Spinner />
      ) : error || !data ? (
        <ErrorState error={error} onRetry={() => refetch()} />
      ) : (
        <div className="card">
          <pre
            className="mono"
            style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word', fontSize: 12 }}
          >
            {JSON.stringify(data, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}
