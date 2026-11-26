import { Column, Entity, PrimaryColumn } from 'typeorm';

@Entity('soroban_indexer_state')
export class SorobanIndexerState {
  @PrimaryColumn({ type: 'varchar', length: 64 })
  id: string;

  @Column({ type: 'integer' })
  latestLedger: number;

  @Column({ type: 'integer', nullable: true })
  cursorStartLedger: number | null;

  @Column({ type: 'text', nullable: true })
  pagingCursor: string | null;

  /**
   * Highest ledger the indexer has fully processed. Used to compute the
   * `soroban_indexer_lag_ledgers` Prometheus gauge and to trigger alerts
   * when the indexer falls behind the chain head (issue #1587).
   */
  @Column({ type: 'integer', nullable: true })
  indexedLedger: number | null;
}
