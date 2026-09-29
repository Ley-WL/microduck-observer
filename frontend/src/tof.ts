export function validTof(data: any): boolean {
  return data?.rows === 8 && data?.cols === 8 &&
    Array.isArray(data.distanceMm) && data.distanceMm.length === 64 &&
    data.distanceMm.every((n: unknown) => Number.isInteger(n) && Number(n) >= 0 && Number(n) <= 65535) &&
    Array.isArray(data.status) && data.status.length === 64 &&
    data.status.every((n: unknown) => Number.isInteger(n) && Number(n) >= 0 && Number(n) <= 255) &&
    Number.isFinite(data.hz) && data.hz >= 0;
}

export function tofStats(data: { distanceMm: number[]; status: number[] } | undefined) {
  const distances = data?.distanceMm.filter((_, i) => data.status[i] === 5).sort((a,b) => a-b) ?? [];
  const n = distances.length;
  return { count: n, min: n ? distances[0] : null, max: n ? distances[n-1] : null,
    median: n ? (distances[Math.floor((n-1)/2)] + distances[Math.floor(n/2)]) / 2 : null };
}
