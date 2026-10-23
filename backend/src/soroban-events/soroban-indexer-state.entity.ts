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
}
