// Naive local instants, the way every boundary in this repository spells
// them: `YYYY-MM-DDTHH:MM:SS`, no zone (SPEC §2).
//
// Arithmetic is done on the digits as if they were UTC, so adding a day is
// always 24 hours of the digits and never a daylight-saving jump the stored
// data knows nothing about.

/** Milliseconds, reading a naive instant's digits as UTC. */
export type Naive = number & { readonly naive: unique symbol };

const HOUR = 3_600_000;
export const DAY = 24 * HOUR;

export function naive(iso: string): Naive {
  const ms = Date.parse(iso.length === 10 ? `${iso}T00:00:00Z` : `${iso.slice(0, 23)}Z`);
  if (Number.isNaN(ms)) throw new Error(`not an instant: ${JSON.stringify(iso)}`);
  return ms as Naive;
}

export function iso(at: Naive): string {
  return new Date(at).toISOString().slice(0, 19);
}

export function plus(at: Naive, ms: number): Naive {
  return (at + ms) as Naive;
}

/** The reader's wall clock, as a naive instant — what `prodrome list` reads. */
export function wall(date: Date): Naive {
  return Date.UTC(
    date.getFullYear(), date.getMonth(), date.getDate(),
    date.getHours(), date.getMinutes(), date.getSeconds(),
  ) as Naive;
}
