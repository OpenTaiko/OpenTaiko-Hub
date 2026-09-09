// Version helpers for OpenTaiko's <main>.<major>.<minor>.<revision> scheme.
// Three-part versions are accepted (revision defaults to 0).

export type ParsedVersion = [main: number, major: number, minor: number, revision: number];

export const parseVersion = (version: unknown): ParsedVersion | null => {
    if (typeof version !== 'string') return null;
    const match = version.trim().match(/^(\d+)\.(\d+)\.(\d+)(?:\.(\d+))?$/);
    if (!match) return null;
    return [Number(match[1]), Number(match[2]), Number(match[3]), Number(match[4] ?? 0)];
}

/** -1, 0 or 1; null when either side is not a version. */
export const compareVersions = (a: unknown, b: unknown): -1 | 0 | 1 | null => {
    const pa = parseVersion(a);
    const pb = parseVersion(b);
    if (!pa || !pb) return null;
    for (let i = 0; i < 4; i++) {
        if (pa[i] !== pb[i]) return pa[i] < pb[i] ? -1 : 1;
    }
    return 0;
}

export const isVersionAtLeast = (version: unknown, target: string): boolean => {
    const cmp = compareVersions(version, target);
    return cmp !== null && cmp >= 0;
}

export const isVersionBelow = (version: unknown, target: string): boolean => {
    const cmp = compareVersions(version, target);
    return cmp !== null && cmp < 0;
}

// True when a version belongs to a given release series, ignoring the revision.
// isVersionInSeries('0.6.0.107', 0, 6, 0) === true
export const isVersionInSeries = (version: unknown, main: number, major: number, minor: number): boolean => {
    const parsed = parseVersion(version);
    if (!parsed) return false;
    return parsed[0] === main && parsed[1] === major && parsed[2] === minor;
}
