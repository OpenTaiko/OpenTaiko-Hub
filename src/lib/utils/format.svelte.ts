// Numbers and sizes as the Hub's chosen language writes them, units included ("850 Mo"
// in French), rather than the system's or English. The language is read through
// fromStore, so text formatted here follows a change of language at once: read with
// get(), it kept the previous language's separators until something else redrew it.
import { fromStore } from 'svelte/store';
import { locale } from 'svelte-i18n';

const language = fromStore(locale);
const current = (): string | undefined => language.current ?? undefined;

export const formatNumber = (value: number, fractionDigits = 0): string =>
    new Intl.NumberFormat(current(), { maximumFractionDigits: fractionDigits, minimumFractionDigits: fractionDigits }).format(value);

const withUnit = (value: number, unit: 'kilobyte' | 'megabyte' | 'gigabyte', fractionDigits: number): string =>
    new Intl.NumberFormat(current(), {
        style: 'unit',
        unit,
        unitDisplay: 'short',
        maximumFractionDigits: fractionDigits,
        minimumFractionDigits: fractionDigits
    }).format(value);

/** A size given in megabytes: "850 MB", "1.2 GB". */
export const formatSize = (mb: number): string =>
    mb >= 1024 ? withUnit(mb / 1024, 'gigabyte', 1) : withUnit(Math.max(1, Math.round(mb)), 'megabyte', 0);

/** A size given in bytes, down to KB: what tells a slow transfer apart from a stuck one. */
export const formatBytes = (n: number): string => {
    if (n >= 1024 ** 3) return withUnit(n / 1024 ** 3, 'gigabyte', 2);
    if (n >= 1024 ** 2) return withUnit(n / 1024 ** 2, 'megabyte', 1);
    return withUnit(Math.round(n / 1024), 'kilobyte', 0);
};

export const formatDateTime = (date: Date): string => date.toLocaleString(current());
