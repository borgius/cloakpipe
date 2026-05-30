import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError, api } from '../api/client';

function mockFetch(status: number, body: unknown, contentType = 'application/json') {
  return vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    headers: { get: () => contentType },
    json: async () => body,
    text: async () => (typeof body === 'string' ? body : JSON.stringify(body)),
  } as unknown as Response);
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('api client', () => {
  it('fetches and parses JSON responses', async () => {
    const status = { service: 'cloakpipe', mode: 'server' };
    vi.stubGlobal('fetch', mockFetch(200, status));

    const result = await api.getSystem();
    expect(result.mode).toBe('server');
    expect(fetch).toHaveBeenCalledWith(
      expect.stringContaining('/admin/api/system'),
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('throws ApiError with code/message on error responses', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetch(404, { error: { code: 'not_found', message: 'Unknown profile' } }),
    );

    await expect(api.getProfile('nope')).rejects.toMatchObject({
      status: 404,
      code: 'not_found',
      message: 'Unknown profile',
    });
    await expect(api.getProfile('nope')).rejects.toBeInstanceOf(ApiError);
  });

  it('serialises bodies for mutations and encodes path params', async () => {
    const fetchMock = mockFetch(200, []);
    vi.stubGlobal('fetch', fetchMock);

    await api.updateRule('a/b', { name: 'x', regex: '\\d+', category: 'NUM' });

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toContain('/categories/rules/a%2Fb');
    expect(init.method).toBe('PUT');
    expect(JSON.parse(init.body)).toEqual({ name: 'x', regex: '\\d+', category: 'NUM' });
  });

  it('builds query strings and drops empty values', async () => {
    const fetchMock = mockFetch(200, { events: [], supported: true });
    vi.stubGlobal('fetch', fetchMock);

    await api.queryAudit({ event: 'detect', surface: '', limit: 10 });

    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toContain('event=detect');
    expect(url).toContain('limit=10');
    expect(url).not.toContain('surface=');
  });
});
