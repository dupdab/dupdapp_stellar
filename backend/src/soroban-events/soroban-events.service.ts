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

const INDEXER_STATE_ID = 'contract-events';
const PAGE_SIZE = 100;

@Injectable()
export class SorobanEventsService implements OnModuleInit, OnModuleDestroy {
  private readonly logger = new Logger(SorobanEventsService.name);
  private readonly rpcUrl: string;
  private readonly contractIds: string[];
  private readonly pollIntervalMs: number;
  private readonly initialLedger: number;
  private timer: ReturnType<typeof setInterval> | undefined;
  private polling = false;
  private requestId = 0;

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
      throw new Error(`Soroban RPC returned HTTP ${response.status}`);
    }

    const body = (await response.json()) as RpcResponse;
    if (body.error) {
      throw new Error(`Soroban RPC ${body.error.code}: ${body.error.message}`);
    }
    if (
      !body.result ||
      !Array.isArray(body.result.events) ||
      !Number.isSafeInteger(body.result.latestLedger)
    ) {
      throw new Error('Soroban RPC returned an invalid getEvents response');
    }
    return body.result;
  }
}
