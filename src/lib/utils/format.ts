// Numbers as the Hub's chosen language writes them, rather than the system's or English.
import { get } from 'svelte/store';
import { locale } from 'svelte-i18n';

const current = (): string | undefined => get(locale) ?? undefined;

export const formatNumber = (value: number, fractionDigits = 0): string =>
    new Intl.NumberFormat(current(), { maximumFractionDigits: fractionDigits, minimumFractionDigits: fractionDigits }).format(value);

/** A size given in megabytes: "850 MB", "1.2 GB". */
export const formatSize = (mb: number): string =>
    mb >= 1024 ? `${formatNumber(mb / 1024, 1)} GB` : `${formatNumber(Math.max(1, Math.round(mb)))} MB`;

/** A size given in bytes, down to KB: what tells a slow transfer apart from a stuck one. */
export const formatBytes = (n: number): string => {
    if (n >= 1024 ** 3) return `${formatNumber(n / 1024 ** 3, 2)} GB`;
    if (n >= 1024 ** 2) return `${formatNumber(n / 1024 ** 2, 1)} MB`;
    return `${formatNumber(Math.round(n / 1024))} KB`;
};

export const formatDateTime = (date: Date): string => date.toLocaleString(current());
