// FROM OBJECTS TO THE DATA A PAGE TYPESETS — one pure function, run in the
// browser against the core compiled to WebAssembly.
//
// Nothing here evaluates FPL. Every number is one the core answered:
// `entries` composes the rows (SPEC §6.7), `fold` gives each todo's function
// and the environment, `link` closes a function over the others, and
// `fulfillment`, `series_knots` and `explain` read it. What this file adds is
// WHICH instants to ask about — the 72 hours of the ring and the 30 days of
// the bar — and the shape `typst/lib.typ` reads.

import { DAY, HOUR, iso, naive, plus, type Naive } from "./time";

/** The part of prodrome-wasm this reads — the glue module satisfies it. */
export interface Core {
  entries(objects: string, at: string | null | undefined, untrusted: string): string;
  fold(objects: string, at: string | null | undefined, untrusted: string): string;
  link(term: string, specs: string): string;
  fulfillment(term: string, now: string, env: string): number;
  explain(term: string, now: string, env: string): string;
  series_knots(term: string, from: string, to: string, history: string): string;
}

/** One object as the site serves it: its name and its canonical print. */
export interface ObjectIn {
  hash: string;
  text: string;
}

/** An §6.7 row, as prodrome-wasm's `entries` puts it on the wire. */
export interface Entry {
  todo: string;
  state: string;
  at: string;
  claimed: string;
  value: number | null;
  unlinked: string | null;
  unconfirmed: boolean;
  conflicts: Record<string, string[]>;
  content: string | null;
  spec: string | null;
  stream: string[];
}

/** One event on a todo's timeline, from `fold`'s `stream`. */
export interface Marker {
  at: string;
  kind: string;
  actor: string;
  hash: string;
}

export interface Marks {
  /** Fulfillment at now and each hour after, `RING_HOURS` of them. */
  ring: number[];
  /** One value per day; the cells before `now` are the past. */
  bar: { values: number[]; now: number };
}

/** What `typst/lib.typ` reads — see typst/README.md. */
export interface View {
  at: string;
  entries: Entry[];
  records: Record<string, Record<string, unknown>>;
  /** Per todo, its first `Created`'s instant and its last one's text. */
  created: Record<string, { at: string; text: string }>;
  marks: Record<string, Marks>;
  explain: Record<string, unknown>;
  history: Record<string, Marker[]>;
  /** The todo a page is about, if it is about one. */
  focus: string | null;
}

export const RING_HOURS = 72;
export const BAR_DAYS = 30;
export const BAR_PAST = 10;

interface Fold {
  env: unknown;
  flatten: Record<string, unknown>;
  history: unknown;
  stream: Record<string, Marker[]>;
}

interface Knots {
  knots: [string, number][];
  exact: boolean;
}

/** A curve read off its knots, straight between them, flat past the ends. */
function between(knots: [Naive, number][], at: Naive): number {
  if (knots.length === 0) return NaN;
  if (at <= knots[0][0]) return knots[0][1];
  for (let i = 1; i < knots.length; i++) {
    const [t1, v1] = knots[i];
    if (at <= t1) {
      const [t0, v0] = knots[i - 1];
      return t1 === t0 ? v1 : v0 + ((v1 - v0) * (at - t0)) / (t1 - t0);
    }
  }
  return knots[knots.length - 1][1];
}

/**
 * The marks for one closed term. The future is asked of `fulfillment`
 * directly, under the environment as of now (nothing later is known); the
 * past is read off `series_knots`, which evaluates each knot against the
 * environment AS OF that knot — a dependency done yesterday was not done a
 * week ago.
 */
function marksOf(core: Core, closed: string, now: Naive, env: string, history: string): Marks {
  const ring: number[] = [];
  for (let h = 0; h < RING_HOURS; h++) {
    ring.push(core.fulfillment(closed, iso(plus(now, h * HOUR)), env));
  }
  const from = plus(now, -BAR_PAST * DAY);
  const past = JSON.parse(core.series_knots(closed, iso(from), iso(now), history)) as Knots;
  const knots = past.knots.map(([at, v]) => [naive(at), v] as [Naive, number]);
  const values: number[] = [];
  for (let d = 0; d < BAR_DAYS; d++) {
    const middle = plus(from, d * DAY + DAY / 2);
    values.push(d < BAR_PAST ? between(knots, middle) : core.fulfillment(closed, iso(middle), env));
  }
  return { ring, bar: { values, now: BAR_PAST } };
}

/**
 * Everything a page needs, read at `now`. `focus` names the one todo whose
 * explanation is wanted (the item page); the list asks for none.
 *
 * Throws what the core throws: objects that do not hash to their names, or do
 * not form a DAG, have no honest reading (see prodrome-wasm's `Read`).
 */
export function read(core: Core, objects: ObjectIn[], now: Naive, focus: string | null): View {
  const sent = JSON.stringify(objects);
  const at = iso(now);
  const rows = JSON.parse(core.entries(sent, at, "[]")) as {
    at: string;
    entries: Entry[];
    records: View["records"];
    created: View["created"];
  };
  const fold = JSON.parse(core.fold(sent, at, "[]")) as Fold;
  const specs = JSON.stringify(fold.flatten);
  const env = JSON.stringify(fold.env);
  const history = JSON.stringify(fold.history);

  const marks: View["marks"] = {};
  const explain: View["explain"] = {};
  for (const [todo, term] of Object.entries(fold.flatten)) {
    // `link` refuses an unknown todo or a loop; that todo then has no marks,
    // and its row already says why it has no value (`unlinked`).
    let closed: string;
    try {
      closed = core.link(JSON.stringify(term), specs);
    } catch {
      continue;
    }
    marks[todo] = marksOf(core, closed, now, env, history);
    if (todo === focus) explain[todo] = JSON.parse(core.explain(closed, at, env));
  }

  return {
    at: rows.at,
    entries: rows.entries,
    records: rows.records,
    created: rows.created,
    marks,
    explain,
    history: fold.stream,
    focus,
  };
}
