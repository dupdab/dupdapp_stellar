import { NestFactory, Reflector } from '@nestjs/core';
import { ClassSerializerInterceptor } from '@nestjs/common';
import { DocumentBuilder, SwaggerModule } from '@nestjs/swagger';
import { Counter, Histogram, Registry, collectDefaultMetrics } from 'prom-client';
import { AppModule } from './app.module';

const metricsRegistry = new Registry();
collectDefaultMetrics({ register: metricsRegistry });

export const contractInvocationsTotal = new Counter({
  name: 'contract_invocations_total',
  help: 'Total number of contract invocations',
  labelNames: ['contract', 'function'] as const,
  registers: [metricsRegistry],
});

export const contractInvocationDurationSeconds = new Histogram({
  name: 'contract_invocation_duration_seconds',
  help: 'Contract invocation latency in seconds',
  labelNames: ['contract', 'function'] as const,
  registers: [metricsRegistry],
});

export const contractErrorsTotal = new Counter({
  name: 'contract_errors_total',
  help: 'Total number of contract invocation errors',
  labelNames: ['error_type'] as const,
  registers: [metricsRegistry],
});

async function bootstrap() {
  const app = await NestFactory.create(AppModule);

  app.setGlobalPrefix('api/v1');
  app.useGlobalInterceptors(new ClassSerializerInterceptor(app.get(Reflector)));

  const httpAdapter = app.getHttpAdapter();
  httpAdapter.get('/metrics', async (_req: unknown, res: any) => {
    res.setHeader('Content-Type', metricsRegistry.contentType);
    res.end(await metricsRegistry.metrics());
  });

  const config = new DocumentBuilder()
    .setTitle('DupDapp API')
    .setDescription('Merchant registration, API key auth, waitlist export & GDPR')
    .setVersion('1.0')
    .addBearerAuth()
    .addApiKey({ type: 'apiKey', in: 'header', name: 'x-api-key' }, 'x-api-key')
    .build();

  SwaggerModule.setup('docs', app, SwaggerModule.createDocument(app, config));

  await app.listen(process.env.PORT ?? 3000);
}

bootstrap();
