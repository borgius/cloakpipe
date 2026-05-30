import type { components } from './schema';

export type Schemas = components['schemas'];

export type SystemStatus = Schemas['SystemStatus'];
export type ProfileSummary = Schemas['ProfileSummary'];
export type ProfileDetail = Schemas['ProfileDetail'];
export type CustomProfileInput = Schemas['CustomProfileInput'];
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

const ADMIN_TOKEN_STORAGE_KEY = 'cloakpipe.adminToken';
const scheme = 'Bearer';

/**
 * The optional admin bearer token. When set, it is attached as an
 * `Authorization` header on every request and persisted to
 * localStorage so it survives reloads. The admin API only requires this when
 * the server is started with `CLOAKPIPE_ADMIN_TOKEN` set.
 */
let adminToken: string | null = readStoredToken();

function readStoredToken(): string | null {
  try {
    return window.localStorage.getItem(ADMIN_TOKEN_STORAGE_KEY);
  } catch {
    return null;
  }
}

export function getAdminToken(): string | null {
  return adminToken;
}

export function setAdminToken(token: string | null): void {
  adminToken = token && token.trim() !== '' ? token.trim() : null;
  try {
    if (adminToken) {
      window.localStorage.setItem(ADMIN_TOKEN_STORAGE_KEY, adminToken);
    } else {
      window.localStorage.removeItem(ADMIN_TOKEN_STORAGE_KEY);
    }
  } catch {
    // localStorage unavailable (e.g. private mode) — keep in-memory only.
  }
}

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
  const headers: Record<string, string> = {};
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (adminToken) headers['Authorization'] = scheme + ' ' + adminToken;
  const res = await fetch(buildUrl(path, query), {
    method,
    signal,
    headers: Object.keys(headers).length > 0 ? headers : undefined,
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
  createProfile: (input: CustomProfileInput) =>
    apiRequest<ProfileDetail>('/profiles', { method: 'POST', body: input }),
  updateProfile: (name: string, input: CustomProfileInput) =>
    apiRequest<ProfileDetail>(`/profiles/${encodeURIComponent(name)}`, {
      method: 'PUT',
      body: input,
    }),
  deleteProfile: (name: string) =>
    apiRequest<Record<string, unknown>>(`/profiles/${encodeURIComponent(name)}`, {
      method: 'DELETE',
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
