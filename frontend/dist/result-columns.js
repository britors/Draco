export const MIN_RESULT_COLUMN_WIDTH = 80;
export const MAX_RESULT_COLUMN_WIDTH = 640;

export function clampResultColumnWidth(width) {
  const numeric = Number(width);
  if (!Number.isFinite(numeric)) return MIN_RESULT_COLUMN_WIDTH;
  return Math.min(MAX_RESULT_COLUMN_WIDTH, Math.max(MIN_RESULT_COLUMN_WIDTH, Math.round(numeric)));
}
