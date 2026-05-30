import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { OverviewPage } from './Overview';
import * as client from '../api/client';

function renderWithQuery(ui: React.ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

describe('OverviewPage', () => {
  it('renders runtime status from the API', async () => {
    vi.spyOn(client.api, 'getSystem').mockResolvedValue({
      service: 'cloakpipe',
      version: '0.18.0',
      mode: 'server',
      listen: '127.0.0.1:8400',
      upstream: 'https://api.openai.com',
      active_profile: 'legal',
      config_path: '/etc/cloakpipe.toml',
      policies_dir: '/etc/policies',
      masking_strategy: 'token',
      detection: {
        secrets: true,
        financial: true,
        dates: false,
        emails: true,
        phone_numbers: false,
        ip_addresses: false,
        urls_internal: false,
        custom_pattern_count: 2,
      },
      ner: {
        enabled: true,
        backend: 'distilbert-pii',
        model: null,
        confidence_threshold: 0.4,
        sidecar_url: 'http://127.0.0.1:9111',
        entity_types: [],
      },
      audit: {
        enabled: true,
        backend: 'jsonl',
        location: '/var/audit',
        log_entities: false,
        retention_days: 90,
      },
      vault: {
        backend: 'file',
        path: './vault.enc',
        persistent: true,
        encryption: 'aes-256-gcm',
        total_mappings: 7,
      },
    });

    renderWithQuery(<OverviewPage />);

    await waitFor(() => expect(screen.getByText('legal')).toBeInTheDocument());
    expect(screen.getByText('Overview')).toBeInTheDocument();
    // Vault mappings stat
    expect(screen.getByText('7')).toBeInTheDocument();
  });

  it('warns when not in server mode', async () => {
    vi.spyOn(client.api, 'getSystem').mockResolvedValue({
      service: 'cloakpipe',
      version: '0.18.0',
      mode: 'proxy',
      listen: '127.0.0.1:8400',
      upstream: 'https://api.openai.com',
      active_profile: null,
      config_path: null,
      policies_dir: null,
      masking_strategy: 'token',
      detection: {
        secrets: true,
        financial: true,
        dates: true,
        emails: true,
        phone_numbers: false,
        ip_addresses: false,
        urls_internal: false,
        custom_pattern_count: 0,
      },
      ner: {
        enabled: false,
        backend: 'distilbert-pii',
        model: null,
        confidence_threshold: 0.4,
        sidecar_url: 'http://127.0.0.1:9111',
        entity_types: [],
      },
      audit: {
        enabled: false,
        backend: 'disabled',
        location: null,
        log_entities: false,
        retention_days: 0,
      },
      vault: {
        backend: 'memory',
        path: null,
        persistent: false,
        encryption: 'none',
        total_mappings: 0,
      },
    });

    renderWithQuery(<OverviewPage />);
    await waitFor(() =>
      expect(screen.getByText(/admin API requires/i)).toBeInTheDocument(),
    );
  });
});
