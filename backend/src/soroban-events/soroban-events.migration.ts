import { MigrationInterface, QueryRunner } from 'typeorm';

export class CreateSorobanEventIndexerTables1760000000000 implements MigrationInterface {
  name = 'CreateSorobanEventIndexerTables1760000000000';

  async up(queryRunner: QueryRunner): Promise<void> {
    await queryRunner.query(`
      CREATE TABLE IF NOT EXISTS "soroban_indexed_events" (
        "id" varchar(255) PRIMARY KEY,
        "contractId" varchar(56) NOT NULL,
        "ledger" integer NOT NULL,
        "transactionHash" varchar(64) NOT NULL,
        "topics" jsonb NOT NULL,
        "value" jsonb NOT NULL,
        "rawEvent" jsonb NOT NULL
      )
    `);
    await queryRunner.query(`
      CREATE INDEX IF NOT EXISTS "IDX_soroban_indexed_events_contract_ledger"
      ON "soroban_indexed_events" ("contractId", "ledger")
    `);
    await queryRunner.query(`
      CREATE TABLE IF NOT EXISTS "soroban_indexer_state" (
        "id" varchar(64) PRIMARY KEY,
        "latestLedger" integer NOT NULL,
        "cursorStartLedger" integer,
        "pagingCursor" text
      )
    `);
  }

  async down(queryRunner: QueryRunner): Promise<void> {
    await queryRunner.query('DROP TABLE "soroban_indexer_state"');
    await queryRunner.query('DROP INDEX "IDX_soroban_indexed_events_contract_ledger"');
    await queryRunner.query('DROP TABLE "soroban_indexed_events"');
  }
}
