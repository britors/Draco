// Pure helpers for the Administration replication monitor, kept apart from app.js so they can be
// tested without a DOM. Values arrive from PostgreSQL as bigint byte counts and seconds as text.

const BYTE_UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];

// Formats a WAL distance in bytes; `null` (hidden without pg_monitor, or no position yet) is '—'.
export function formatWalBytes(bytes, locale = 'en') {
  if (bytes === null || bytes === undefined || !Number.isFinite(Number(bytes))) return '—';
  let value = Math.max(0, Number(bytes));
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: digits, minimumFractionDigits: digits }).format(value)} ${BYTE_UNITS[unit]}`;
}

// Formats seconds reported as text (e.g. '0.002', '3600.0'); `null` is '—'.
export function formatLagSeconds(seconds, locale = 'en') {
  if (seconds === null || seconds === undefined || seconds === '') return '—';
  const value = Number(seconds);
  if (!Number.isFinite(value)) return '—';
  const format = (number, digits) => new Intl.NumberFormat(locale, { maximumFractionDigits: digits }).format(number);
  if (value < 1) return `${format(value * 1000, 0)} ms`;
  if (value < 120) return `${format(value, 1)} s`;
  if (value < 7200) return `${format(value / 60, 1)} min`;
  return `${format(value / 3600, 1)} h`;
}

// An inactive slot keeps WAL on disk until something consumes it; a lost slot is broken.
export function slotNeedsAttention(slot) {
  return slot.wal_status === 'lost' || slot.wal_status === 'unreserved' || (!slot.active && Number(slot.retained_wal_bytes) > 0);
}

// Picks which state the panel shows: 'standby', 'primary' (has senders or slots) or 'none'.
export function replicationMode(status) {
  if (status.in_recovery) return 'standby';
  return status.replicas.length || status.slots.length ? 'primary' : 'none';
}
