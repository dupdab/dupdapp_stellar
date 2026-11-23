import { Module } from '@nestjs/common';
import { TypeOrmModule } from '@nestjs/typeorm';
import { SorobanIndexedEvent } from './soroban-indexed-event.entity';
import { SorobanIndexerState } from './soroban-indexer-state.entity';
import { SorobanEventsService } from './soroban-events.service';
import { SorobanIndexerLagService } from './soroban-indexer-lag.service';

@Module({
  imports: [TypeOrmModule.forFeature([SorobanIndexedEvent, SorobanIndexerState])],
  providers: [SorobanEventsService, SorobanIndexerLagService],
  exports: [SorobanIndexerLagService],
})
export class SorobanEventsModule {}
