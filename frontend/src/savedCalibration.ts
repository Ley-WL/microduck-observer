// Storage is scoped to the backend and data source, never shared between robots.
export function calibrationKey(kind: string, context: unknown[]) {
  return "microduck-calibration-v1:" + kind + ":" + JSON.stringify(context);
}
export function loadCalibration(key: string): any {
  try { return JSON.parse(localStorage.getItem(key) || "null"); }
  catch { return null; }
}
export function saveCalibration(key: string, value: unknown): boolean {
  try { localStorage.setItem(key, JSON.stringify(value)); return true; }
  catch { return false; }
}
