import type { components } from './schema';

export type Schemas = components['schemas'];

export type SystemStatus = Schemas['SystemStatus'];
export type ProfileSummary = Schemas['ProfileSummary'];
export type ProfileDetail = Schemas['ProfileDetail'];
export type PolicySummary = Schemas['PolicySummary'];
export type PolicyDetail = Schemas['PolicyDetail'];
export type PolicyContent = Schemas['PolicyContent'];
export type PolicyActivation = Schemas['PolicyActivation'];
export type ValidationReport = Schemas['ValidationReport'];
export type CategoriesResponse = Schemas['CategoriesResponse'];
export type CustomPattern = Schemas['CustomPattern'];
export type DetectionFamily = Schemas['DetectionFamily'];
export type AuditEntry = Schemas['AuditEntry'];
export type AuditEventsResponse = Schemas['AuditEventsResponse'];
export type AuditSummaryResponse = Schemas['AuditSummaryResponse'];
export type VaultStatsResponse = Schemas['VaultStatsResponse'];
export type MappingsResponse = Schemas['MappingsResponse'];
export type MappingEntry = Schemas['MappingEntry'];

/**
 * Base URL of the CloakPipe server-mode instance. Defaults to same-origin so the
 * SPA works when served by `npx cloakpipe serve` (which reverse-proxies the
 * backend). For standalone dev, set VITE_CLOAKPIPE_BASE_URL.
 */
export const BASE_URL: string =
  (import.meta.env.VITE_CLOAKPIPE_BASE_URL as string | undefined)?.replace(/\/$/, '') ?? '';

const API_PREFIX = '/admin/api';

export class ApiError extends Error {
  status: number;
  code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
    this.name = 'ApiError';
  }
}

interface RequestOptions {
  method?: string;
  body?: unknown;
  query?: Record<string, string | number | boolean | undefined>;
  signal?: AbortSignal;
}

function buildUrl(path: string, query?: RequestOptions['query']): string {
  const url = `${BASE_URL}${API_PREFIX}${path}`;
  if (!query) return url;
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined && value !== '') params.set(key, String(value));
  }
  const qs = params.toString();
  return qs ? `${url}?${qs}` : url;
}

export async function apiRequest<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { method = 'GET', body, query, signal } = options;
  const res = await fetch(buildUrl(path, query), {
    method,
    signal,
    headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  if (!res.ok) {
    let code = 'http_error';
    let message = `Request failed (${res.status})`;
    try {
      const data = await res.json();
      if (data?.error) {
        code = data.error.code ?? code;
        message = data.error.message ?? message;
      }
    } catch {
      // non-JSON error body
    }
    throw new ApiError(res.status, code, message);
  }

  if (res.status === 204) return undefined as T;
  const contentType = res.headers.get('content-type') ?? '';
  if (contentType.includes('application/json')) {
    return (await res.json()) as T;
  }
  return (await res.text()) as unknown as T;
}

/** Typed admin API surface. */
export const api = {
  getSystem: () => apiRequest<SystemStatus>('/system'),
  listSessions: () => apiRequest<Record<string, unknown>>('/sessions'),

  listProfiles: () => apiRequest<ProfileSummary[]>('/profiles'),
  getProfile: (name: string) => apiRequest<ProfileDetail>(`/profiles/${encodeURIComponent(name)}`),
  activateProfile: (name: string) =>
    apiRequest<ProfileDetail>(`/profiles/${encodeURIComponent(name)}/activate`, {
      method: 'POST',
    }),

  listPolicies: () => apiRequest<PolicySummary[]>('/policies'),
  getPolicy: (name: string) => apiRequest<PolicyDetail>(`/policies/${encodeURIComponent(name)}`),
  putPolicy: (name: string, content: string) =>
    apiRequest<PolicyDetail>(`/policies/${encodeURIComponent(name)}`, {
      method: 'PUT',
      body: { content } satisfies PolicyContent,
    }),
  deletePolicy: (name: string) =>
    apiRequest<Record<string, unknown>>(`/policies/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    }),
  activatePolicy: (name: string) =>
    apiRequest<PolicyActivation>(`/policies/${encodeURIComponent(name)}/activate`, {
      method: 'POST',
    }),
  validatePolicy: (content: string) =>
    apiRequest<ValidationReport>('/policy/validate', {
      method: 'POST',
      body: { content } satisfies PolicyContent,
    }),

  listCategories: () => apiRequest<CategoriesResponse>('/categories'),
  createRule: (rule: CustomPattern) =>
    apiRequest<CustomPattern[]>('/categories/rules', { method: 'POST', body: rule }),
  updateRule: (name: string, rule: CustomPattern) =>
    apiRequest<CustomPattern[]>(`/categories/rules/${encodeURIComponent(name)}`, {
      method: 'PUT',
      body: rule,
    }),
  deleteRule: (name: string) =>
    apiRequest<CustomPattern[]>(`/categories/rules/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    }),

  queryAudit: (query: RequestOptions['query']) =>
    apiRequest<AuditEventsResponse>('/audit/events', { query }),
  auditSummary: () => apiRequest<AuditSummaryResponse>('/audit/summary'),
  auditExportUrl: () => `${BASE_URL}${API_PREFIX}/audit/export`,

  vaultStats: () => apiRequest<VaultStatsResponse>('/vault/stats'),
  vaultMappings: (query: RequestOptions['query']) =>
    apiRequest<MappingsResponse>('/vault/mappings', { query }),
};
