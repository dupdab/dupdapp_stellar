import { Injectable, Logger, OnModuleDestroy, OnModuleInit } from '@nestjs/common';
import { ConfigService } from '@nestjs/config';
import { InjectRepository } from '@nestjs/typeorm';
import { Repository } from 'typeorm';
import { SorobanIndexedEvent } from './soroban-indexed-event.entity';
import { SorobanIndexerState } from './soroban-indexer-state.entity';

interface RpcEvent {
  pagingToken: string;
  type: string;
  contractId: string;
  ledger: number;
  txHash: string;
  topic: unknown[];
  value: unknown;
  [key: string]: unknown;
}

interface GetEventsResult {
  events: RpcEvent[];
  latestLedger: number;
}

interface RpcResponse {
  result?: GetEventsResult;
  error?: { code: number; message: string };
}

interface DeploymentEvent {
  contract_name: string;
  wasm_hash: string;
  deployer: string;
  timestamp: string;
}

const INDEXER_STATE_ID = 'contract-events';
const PAGE_SIZE = 100;
const DEPLOYMENT_TOPIC = 'contract_deployed';

@Injectable()
export class SorobanEventsService implements OnModuleInit, OnModuleDestroy {
  private readonly logger = new Logger(SorobanEventsService.name);
  private readonly rpcUrl: string;
  private readonly contractIds: string[];
  private readonly pollIntervalMs: number;
  private readonly initialLedger: number;
  private readonly network: string;
  private readonly adminAlertWebhookUrl: string | undefined;
  private timer: ReturnType<typeof setInterval> | undefined;
  private polling = false;
  private requestId = 0;

  // Observability: contract transaction failure counters (issue #1589).
  private readonly txFailures = new Map<string, number>();
  private txSuccesses = 0;

  // Observability: contract deployment counters (issue #1591).
  private deploymentCount = 0;

  constructor(
    config: ConfigService,
    @InjectRepository(SorobanIndexedEvent)
    private readonly events: Repository<SorobanIndexedEvent>,
    @InjectRepository(SorobanIndexerState)
    private readonly states: Repository<SorobanIndexerState>,
  ) {
    this.rpcUrl = config.get<string>('SOROBAN_RPC_URL') ?? 'https://soroban-testnet.stellar.org';
    this.contractIds = (config.get<string>('SOROBAN_EVENT_CONTRACT_IDS') ?? '')
      .split(',')
      .map((contractId) => contractId.trim())
      .filter(Boolean);

    const pollIntervalMs = Number(config.get<string>('SOROBAN_EVENT_POLL_INTERVAL_MS') ?? 5000);
    const initialLedger = Number(config.get<string>('SOROBAN_EVENT_START_LEDGER') ?? 1);
    if (!Number.isSafeInteger(pollIntervalMs) || pollIntervalMs < 1000) {
      throw new Error('SOROBAN_EVENT_POLL_INTERVAL_MS must be an integer of at least 1000');
    }
    if (!Number.isSafeInteger(initialLedger) || initialLedger < 1) {
      throw new Error('SOROBAN_EVENT_START_LEDGER must be a positive integer');
    }
    this.pollIntervalMs = pollIntervalMs;
    this.initialLedger = initialLedger;
    this.network = config.get<string>('STELLAR_NETWORK') ?? 'testnet';
    this.adminAlertWebhookUrl = config.get<string>('ADMIN_ALERT_WEBHOOK_URL');
  }

  onModuleInit(): void {
    if (this.contractIds.length === 0) {
      this.logger.warn('Event indexing is disabled: SOROBAN_EVENT_CONTRACT_IDS is empty');
      return;
    }
    this.timer = setInterval(() => void this.pollSafely(), this.pollIntervalMs);
    void this.pollSafely();
  }

  onModuleDestroy(): void {
    if (this.timer) clearInterval(this.timer);
  }

  /**
   * Prometheus text exposition for contract transaction outcomes.
   * Exposes `contract_tx_failures_total` labeled by `error_type` and
   * `contract_tx_successes_total` so the failure rate can be computed
   * (failures / (failures + successes)) for alerting and dashboards.
   */
  getMetrics(): string {
    const lines: string[] = [
      '# HELP contract_tx_failures_total Total number of failed contract transactions.',
      '# TYPE contract_tx_failures_total counter',
    ];
    for (const [errorType, count] of this.txFailures) {
      lines.push(`contract_tx_failures_total{error_type="${errorType}"} ${count}`);
    }
    lines.push(
      '# HELP contract_tx_successes_total Total number of successful contract transactions.',
      '# TYPE contract_tx_successes_total counter',
      `contract_tx_successes_total ${this.txSuccesses}`,
      '# HELP contract_deployments_total Total number of captured contract deployment events.',
      '# TYPE contract_deployments_total counter',
      `contract_deployments_total ${this.deploymentCount}`,
    );
    return lines.join('\n');
  }

  private recordTxFailure(errorType: string): void {
    this.txFailures.set(errorType, (this.txFailures.get(errorType) ?? 0) + 1);
  }

  private recordTxSuccess(): void {
    this.txSuccesses += 1;
  }

  /**
   * Captures a structured contract deployment event (issue #1591).
   * Emits a structured log line and, on mainnet, alerts the admin.
   */
  private async recordDeployment(event: RpcEvent): Promise<void> {
    const deployment = this.parseDeployment(event);
    if (!deployment) return;

    this.deploymentCount += 1;
    this.logger.log(
      `Contract deployment captured: ${JSON.stringify({
        contract_name: deployment.contract_name,
        wasm_hash: deployment.wasm_hash,
        deployer: deployment.deployer,
        timestamp: deployment.timestamp,
        contract_id: event.contractId,
        ledger: event.ledger,
        tx_hash: event.txHash,
        network: this.network,
      })}`,
    );

    if (this.network === 'mainnet' || this.network === 'public') {
      await this.alertAdmin(deployment, event);
    }
  }

  private parseDeployment(event: RpcEvent): DeploymentEvent | null {
    const topics = Array.isArray(event.topic) ? event.topic : [];
    const isDeployment = topics.some(
      (topic) => typeof topic === 'string' && topic === DEPLOYMENT_TOPIC,
    );
    if (!isDeployment) return null;

    const value = event.value;
    if (!value || typeof value !== 'object') return null;
    const data = value as Record<string, unknown>;
    const contractName = data.contract_name ?? data.contractName;
    const wasmHash = data.wasm_hash ?? data.wasmHash;
    const deployer = data.deployer;
    const timestamp = data.timestamp;
    if (
      typeof contractName !== 'string' ||
      typeof wasmHash !== 'string' ||
      typeof deployer !== 'string'
    ) {
      return null;
    }
    return {
      contract_name: contractName,
      wasm_hash: wasmHash,
      deployer,
      timestamp:
        typeof timestamp === 'string'
          ? timestamp
          : typeof timestamp === 'number'
            ? new Date(timestamp).toISOString()
            : new Date().toISOString(),
    };
  }

  private async alertAdmin(deployment: DeploymentEvent, event: RpcEvent): Promise<void> {
    const message =
      `Mainnet contract deployment: ${deployment.contract_name} ` +
      `(wasm ${deployment.wasm_hash}) by ${deployment.deployer} at ${deployment.timestamp} ` +
      `[tx ${event.txHash}, ledger ${event.ledger}]`;
    this.logger.warn(`ADMIN ALERT: ${message}`);

    if (!this.adminAlertWebhookUrl) return;
    try {
      const response = await fetch(this.adminAlertWebhookUrl, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          severity: 'critical',
          event: 'contract_deployed',
          network: this.network,
          contract_name: deployment.contract_name,
          wasm_hash: deployment.wasm_hash,
          deployer: deployment.deployer,
          timestamp: deployment.timestamp,
          contract_id: event.contractId,
          ledger: event.ledger,
          tx_hash: event.txHash,
          message,
        }),
      });
      if (!response.ok) {
        this.logger.error(`Admin deployment alert failed: HTTP ${response.status}`);
      }
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      this.logger.error(`Admin deployment alert failed: ${detail}`);
    }
  }

  private async pollSafely(): Promise<void> {
    if (this.polling) return;
    this.polling = true;
    try {
      await this.poll();
    } catch (error) {
      const message = error instanceof Error ? error.stack ?? error.message : String(error);
      this.logger.error(`Soroban event indexing failed: ${message}`);
    } finally {
      this.polling = false;
    }
  }

  private async poll(): Promise<void> {
    let state = await this.states.findOneBy({ id: INDEXER_STATE_ID });
    if (!state) {
      state = this.states.create({
        id: INDEXER_STATE_ID,
        latestLedger: this.initialLedger - 1,
        cursorStartLedger: null,
        pagingCursor: null,
      });
      await this.states.save(state);
    }

    while (true) {
      const startLedger = state.pagingCursor
        ? state.cursorStartLedger
        : state.latestLedger + 1;
      if (startLedger === null) {
        throw new Error('Indexer cursor is missing its start ledger');
      }

      const result = await this.getEvents(startLedger, state.pagingCursor);
      if (result.events.length === 0) {
        state.latestLedger = result.latestLedger;
        state.cursorStartLedger = null;
        state.pagingCursor = null;
        await this.states.save(state);
        return;
      }

      const page = result.events.map((event) => {
        if (!event.pagingToken || !event.contractId || !event.txHash) {
          throw new Error('Soroban RPC returned an event without its paging token or identity');
        }
        return this.events.create({
          id: event.pagingToken,
          contractId: event.contractId,
          ledger: event.ledger,
          transactionHash: event.txHash,
          topics: event.topic,
          value: event.value,
          rawEvent: event as Record<string, unknown>,
        });
      });
      await this.events.upsert(page, ['id']);

      for (const event of result.events) {
        await this.recordDeployment(event);
      }

      const nextCursor = result.events[result.events.length - 1].pagingToken;
      if (!nextCursor) {
        throw new Error('Soroban RPC returned an event without a paging token');
      }
      state.cursorStartLedger = startLedger;
      state.pagingCursor = nextCursor;
      await this.states.save(state);
    }
  }

  private async getEvents(startLedger: number, cursor: string | null): Promise<GetEventsResult> {
    const response = await fetch(this.rpcUrl, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: ++this.requestId,
        method: 'getEvents',
        params: {
          startLedger,
          filters: [{ type: 'contract', contractIds: this.contractIds }],
          pagination: {
            ...(cursor ? { cursor } : {}),
            limit: PAGE_SIZE,
          },
        },
      }),
    });
    if (!response.ok) {
      this.recordTxFailure(`http_${response.status}`);
      throw new Error(`Soroban RPC returned HTTP ${response.status}`);
    }

    const body = (await response.json()) as RpcResponse;
    if (body.error) {
      this.recordTxFailure(`rpc_${body.error.code}`);
      throw new Error(`Soroban RPC ${body.error.code}: ${body.error.message}`);
    }
    if (
      !body.result ||
      !Array.isArray(body.result.events) ||
      !Number.isSafeInteger(body.result.latestLedger)
    ) {
      this.recordTxFailure('invalid_response');
      throw new Error('Soroban RPC returned an invalid getEvents response');
    }
    this.recordTxSuccess();
    return body.result;
  }
}
