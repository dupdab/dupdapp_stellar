import { Injectable, Logger } from '@nestjs/common';
import { Counter, Histogram, Registry, collectDefaultMetrics } from 'prom-client';

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
   * Returns the Prometheus metrics payload for the /metrics endpoint.
   */
  async getMetrics(): Promise<string> {
    return this.registry.metrics();
  }

  get contentType(): string {
    return this.registry.contentType;
  }
}
