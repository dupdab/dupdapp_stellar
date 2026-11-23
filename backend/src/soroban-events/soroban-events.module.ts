import { Module } from '@nestjs/common';
import { TypeOrmModule } from '@nestjs/typeorm';
import { SorobanIndexedEvent } from './soroban-indexed-event.entity';
import { SorobanIndexerState } from './soroban-indexer-state.entity';
import { SorobanEventsService } from './soroban-events.service';
import { ContractTxMetricsService } from './contract-tx-metrics.service';

@Module({
  imports: [TypeOrmModule.forFeature([SorobanIndexedEvent, SorobanIndexerState])],
  providers: [SorobanEventsService, ContractTxMetricsService],
  exports: [SorobanEventsService, ContractTxMetricsService],
})
export class SorobanEventsModule {}
