import { Module, ValidationPipe } from '@nestjs/common';
import { ConfigModule, ConfigService } from '@nestjs/config';
import { TypeOrmModule } from '@nestjs/typeorm';
import { APP_PIPE } from '@nestjs/core';
import { AuthModule } from './auth/auth.module';
import { WaitlistModule } from './waitlist/waitlist.module';
import { AdminModule } from './admin/admin.module';
import { Merchant } from './merchant/merchant.entity';
import { WaitlistEntry } from './waitlist/waitlist.entity';
import { SorobanEventsModule } from './soroban-events/soroban-events.module';
import { SorobanIndexedEvent } from './soroban-events/soroban-indexed-event.entity';
import { SorobanIndexerState } from './soroban-events/soroban-indexer-state.entity';
import { CreateSorobanEventIndexerTables1760000000000 } from './soroban-events/soroban-events.migration';

@Module({
  imports: [
    ConfigModule.forRoot({ isGlobal: true }),
    TypeOrmModule.forRootAsync({
      imports: [ConfigModule],
      useFactory: (config: ConfigService) => ({
        type: 'postgres',
        url: config.get<string>('DATABASE_URL'),
        entities: [Merchant, WaitlistEntry, SorobanIndexedEvent, SorobanIndexerState],
        migrations: [CreateSorobanEventIndexerTables1760000000000],
        migrationsRun: true,
        synchronize: config.get<string>('NODE_ENV') !== 'production',
        ssl: config.get<string>('NODE_ENV') === 'production' ? { rejectUnauthorized: false } : false,
      }),
      inject: [ConfigService],
    }),
    AuthModule,
    WaitlistModule,
    AdminModule,
    SorobanEventsModule,
  ],
  providers: [
    {
      provide: APP_PIPE,
      useValue: new ValidationPipe({ whitelist: true, transform: true }),
    },
  ],
})
export class AppModule {}
