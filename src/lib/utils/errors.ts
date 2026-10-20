import { get } from 'svelte/store';
import { _ } from 'svelte-i18n';

/** The raw text of an error, as thrown by the page or returned by a backend command. */
export const ErrorText = (error: unknown): string =>
    error instanceof Error ? error.message : String(error);

// Failures users meet in practice, recognised from the backend's error text. The text
// itself names a URL or an OS error code, which says nothing to most people; it stays
// available through the toast's copy button and the log.
const KNOWN: [RegExp, string][] = [
    [/HTTP (error status: )?(403|429)\b/, 'errors.rate_limited'],
    [/HTTP (error status: )?404\b/, 'errors.not_found'],
    [/no space left|not enough space/i, 'errors.disk_full'],
    [/access is denied|permission denied|operation not permitted/i, 'errors.access_denied'],
    [/error sending request|dns error|failed to lookup|connection (refused|reset|closed)|timed out|network is unreachable/i, 'errors.offline']
];

// A system error also carries its number ("(os error 5)"), which is what is left to go
// by on Windows, where the words come in the system's language. A number means something
// else on each system (5 is "access denied" on Windows, an input/output error on Linux),
// so the numbers of the system the Hub runs on are used.
const OS_CODES: Record<'windows' | 'unix', [number[], string][]> = {
    windows: [[[39, 112], 'errors.disk_full'], [[5], 'errors.access_denied']],
    unix: [[[28, 122], 'errors.disk_full'], [[1, 13], 'errors.access_denied']]
};

const RUNS_ON_WINDOWS = typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent);

/** The translation key of the error's cause, when it is one of the known ones. */
export const ErrorCause = (error: unknown, windows = RUNS_ON_WINDOWS): string | null => {
    const raw = ErrorText(error);
    const known = KNOWN.find(([pattern]) => pattern.test(raw))?.[1];
    if (known) return known;
    const code = Number(raw.match(/\(os error (\d+)\)/)?.[1]);
    return OS_CODES[windows ? 'windows' : 'unix'].find(([codes]) => codes.includes(code))?.[1] ?? null;
};

/** A message for the user: a plain explanation when the cause is known, else the raw text. */
export const DescribeError = (error: unknown): string => {
    const cause = ErrorCause(error);
    return cause ? get(_)(cause) : ErrorText(error);
};
