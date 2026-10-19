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
    [/no space left|not enough space|os error (28|112)\b/i, 'errors.disk_full'],
    [/access is denied|permission denied|os error (5|13)\b/i, 'errors.access_denied'],
    [/error sending request|dns error|failed to lookup|connection (refused|reset|closed)|timed out|network is unreachable/i, 'errors.offline']
];

/** The translation key of the error's cause, when it is one of the known ones. */
export const ErrorCause = (error: unknown): string | null => {
    const raw = ErrorText(error);
    return KNOWN.find(([pattern]) => pattern.test(raw))?.[1] ?? null;
};

/** A message for the user: a plain explanation when the cause is known, else the raw text. */
export const DescribeError = (error: unknown): string => {
    const cause = ErrorCause(error);
    return cause ? get(_)(cause) : ErrorText(error);
};
