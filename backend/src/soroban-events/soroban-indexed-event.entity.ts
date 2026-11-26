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

  @Column({ type: 'varchar', length: 64, nullable: true })
  eventType: string | null;

  @Column({ type: 'varchar', length: 128, nullable: true })
  contractName: string | null;

  @Column({ type: 'varchar', length: 64, nullable: true })
  wasmHash: string | null;

  @Column({ type: 'varchar', length: 56, nullable: true })
  deployer: string | null;

  @Column({ type: 'varchar', length: 16, nullable: true })
  network: string | null;

  @Column({ type: 'timestamptz', nullable: true })
  deployedAt: Date | null;
}
