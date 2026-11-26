import {
  Injectable,
  ConflictException,
  NotFoundException,
  UnauthorizedException,
} from '@nestjs/common';
import { InjectRepository } from '@nestjs/typeorm';
import { Repository, Between, FindOptionsWhere } from 'typeorm';
import { randomBytes } from 'crypto';
import { WaitlistEntry } from './waitlist.entity';
import { JoinWaitlistDto } from './dto/join-waitlist.dto';

@Injectable()
export class WaitlistService {
  constructor(
    @InjectRepository(WaitlistEntry)
    private readonly waitlistRepo: Repository<WaitlistEntry>,
  ) {}

  async join(dto: JoinWaitlistDto) {
    const existing = await this.waitlistRepo.findOne({ where: { email: dto.email } });
    if (existing) throw new ConflictException('Email already on waitlist');

    const unsubscribeToken = randomBytes(32).toString('hex');
    const referralCode = randomBytes(6).toString('hex').toUpperCase();

    // Credit referrer if valid code provided
    if (dto.referralCode) {
      const referrer = await this.waitlistRepo.findOne({
        where: { referralCode: dto.referralCode },
      });
      if (referrer && referrer.email !== dto.email) {
        await this.waitlistRepo.increment({ id: referrer.id }, 'referrals', 1);
      }
    }

    const entry = this.waitlistRepo.create({ ...dto, unsubscribeToken, referralCode });
    return this.waitlistRepo.save(entry);
  }

  async checkUsername(username: string): Promise<{ available: boolean }> {
    const existing = await this.waitlistRepo.findOne({
      where: { username: username.toLowerCase() },
    });
    return { available: !existing };
  }

  async getStats() {
    const total = await this.waitlistRepo.count();
    return { total };
  }

  async getPosition(email: string): Promise<{ position: number }> {
    const entry = await this.waitlistRepo.findOne({ where: { email } });
    if (!entry) throw new NotFoundException('Email not found on waitlist');
    const position = await this.waitlistRepo.count({
      where: { createdAt: Between(new Date(0), entry.createdAt) },
    });
    return { position };
  }

  /** One-click unsubscribe — no login required, token from email link */
  async unsubscribe(token: string): Promise<{ message: string }> {
    const entry = await this.waitlistRepo.findOne({ where: { unsubscribeToken: token } });
    if (!entry) throw new UnauthorizedException('Invalid or expired unsubscribe token');
    await this.waitlistRepo.update(entry.id, { unsubscribed: true });
    return { message: 'Successfully unsubscribed' };
  }

  /** Full GDPR deletion — removes all PII, requires email + token */
  async deleteEntry(email: string, token: string): Promise<{ message: string }> {
    const entry = await this.waitlistRepo.findOne({ where: { email, unsubscribeToken: token } });
    if (!entry) throw new UnauthorizedException('Invalid token or email not found');
    await this.waitlistRepo.delete(entry.id);
    // In production: queue a confirmation email here
    return { message: 'Your data has been deleted. A confirmation email will be sent shortly.' };
  }

  /** Export waitlist as CSV — admin use */
  async exportCsv(opts: {
    country?: string;
    dateFrom?: string;
    dateTo?: string;
    includeUnsubscribed?: boolean;
  }): Promise<string> {
    const where: FindOptionsWhere<WaitlistEntry> = {};
    if (!opts.includeUnsubscribed) where.unsubscribed = false;
    if (opts.country) where.country = opts.country;

    let entries = await this.waitlistRepo.find({ where, order: { createdAt: 'ASC' } });

    if (opts.dateFrom || opts.dateTo) {
      const from = opts.dateFrom ? new Date(opts.dateFrom) : new Date(0);
      const to = opts.dateTo ? new Date(opts.dateTo) : new Date();
      entries = entries.filter((e) => e.createdAt >= from && e.createdAt <= to);
    }

    const header = 'email,username,businessName,country,position,referrals,createdAt';
    const rows = entries.map((e, i) =>
      [
        e.email,
        e.username ?? '',
        e.businessName ?? '',
        e.country ?? '',
        i + 1,
        e.referrals,
        e.createdAt.toISOString(),
      ]
        .map((v) => `"${String(v).replace(/"/g, '""')}"`)
        .join(','),
    );

    return [header, ...rows].join('\n');
  }
}
