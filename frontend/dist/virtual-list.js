// Pure windowing math for the results grid: given scroll position and a fixed row height, which
// row indexes actually need a DOM node right now. No DOM here so it stays testable without a
// browser; app.js turns the range into real <tr> elements plus two spacer rows.

export function visibleRange({ rowCount, rowHeight, scrollTop, viewportHeight, overscan = 8 }) {
  if (rowCount <= 0 || rowHeight <= 0) return { start: 0, end: 0 };
  const firstVisible = Math.floor(Math.max(0, scrollTop) / rowHeight);
  const visibleCount = Math.max(1, Math.ceil(Math.max(0, viewportHeight) / rowHeight));
  const start = Math.min(rowCount, Math.max(0, firstVisible - overscan));
  const end = Math.min(rowCount, Math.max(start, firstVisible + visibleCount + overscan));
  return { start, end };
}
