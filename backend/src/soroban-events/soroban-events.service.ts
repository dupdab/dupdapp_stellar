import { Injectable, Logger } from '@nestjs/common';
import { Counter, Gauge, Histogram, Registry, collectDefaultMetrics } from 'prom-client';

/**
 * Number of ledgers the Soroban event indexer may fall behind before an alert
 * should fire (indexed_ledger < current_ledger - threshold).
 */
export const SOROBAN_INDEXER_LAG_THRESHOLD = 1000;

@Injectable()
export class SorobanEventsService {
  private readonly logger = new Logger(SorobanEventsService.name);

  private readonly registry = new Registry();

  private readonly contractInvocationsTotal = new Counter({
    name: 'contract_invocations_total',
    help: 'Total number of contract invocations',
    labelNames: ['contract', 'function'] as const,
    registers: [this.registry],
  });

  private readonly contractInvocationDurationSeconds = new Histogram({
    name: 'contract_invocation_duration_seconds',
    help: 'Contract invocation latency in seconds',
    labelNames: ['contract', 'function'] as const,
    registers: [this.registry],
  });

  private readonly contractErrorsTotal = new Counter({
    name: 'contract_errors_total',
    help: 'Total number of contract invocation errors',
    labelNames: ['error_type'] as const,
    registers: [this.registry],
  });

  private readonly sorobanIndexerLagLedgers = new Gauge({
    name: 'soroban_indexer_lag_ledgers',
    help: 'Number of ledgers the Soroban event indexer is behind the current ledger',
    registers: [this.registry],
  });

  constructor() {
    collectDefaultMetrics({ register: this.registry });
  }

  /**
   * Records a contract invocation and its latency.
   */
  recordInvocation(contract: string, fn: string, durationSeconds: number): void {
    this.contractInvocationsTotal.inc({ contract, function: fn });
    this.contractInvocationDurationSeconds.observe({ contract, function: fn }, durationSeconds);
  }

  /**
   * Records a contract invocation error.
   */
  recordError(errorType: string): void {
    this.contractErrorsTotal.inc({ error_type: errorType });
  }

  /**
   * Updates the indexer lag gauge from the indexed and current ledger numbers.
   * Emits a warning log when the indexer falls behind the alert threshold so
   * PagerDuty/Slack routing (see observability alert rules) can be triggered.
   */
  recordIndexerProgress(indexedLedger: number, currentLedger: number): void {
    const lag = Math.max(0, currentLedger - indexedLedger);
    this.sorobanIndexerLagLedgers.set(lag);

    if (indexedLedger < currentLedger - SOROBAN_INDEXER_LAG_THRESHOLD) {
      this.logger.warn(
        `Soroban event indexer is behind: indexed_ledger=${indexedLedger} current_ledger=${currentLedger} lag=${lag} (threshold=${SOROBAN_INDEXER_LAG_THRESHOLD})`,
      );
    }
  }

  /**
   * Returns the Prometheus metrics payload for the /metrics endpoint.
   */
  async getMetrics(): Promise<string> {
    return this.registry.metrics();
  }

  get contentType(): string {
    return this.registry.contentType;
  }
}
