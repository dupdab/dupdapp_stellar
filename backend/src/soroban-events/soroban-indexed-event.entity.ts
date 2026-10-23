import { Column, Entity, PrimaryColumn } from 'typeorm';

@Entity('soroban_indexed_events')
export class SorobanIndexedEvent {
  @PrimaryColumn({ type: 'varchar', length: 255 })
  id: string;

  @Column({ type: 'varchar', length: 56 })
  contractId: string;

  @Column({ type: 'integer' })
  ledger: number;

  @Column({ type: 'varchar', length: 64 })
  transactionHash: string;

  @Column({ type: 'jsonb' })
  topics: unknown[];

  @Column({ type: 'jsonb' })
  value: unknown;

  @Column({ type: 'jsonb' })
  rawEvent: Record<string, unknown>;
}
