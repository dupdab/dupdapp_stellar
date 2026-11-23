import { Module } from '@nestjs/common';
import { TypeOrmModule } from '@nestjs/typeorm';
import { SorobanIndexedEvent } from './soroban-indexed-event.entity';
import { SorobanIndexerState } from './soroban-indexer-state.entity';
import { SorobanEventsService } from './soroban-events.service';
import { ContractRegistryService } from './contract-registry.service';

@Module({
  imports: [TypeOrmModule.forFeature([SorobanIndexedEvent, SorobanIndexerState])],
  providers: [SorobanEventsService, ContractRegistryService],
  exports: [ContractRegistryService],
})
export class SorobanEventsModule {}
