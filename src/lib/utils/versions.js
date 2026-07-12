// Version helpers for OpenTaiko's <main>.<major>.<minor>.<revision> scheme.
// Three-part versions are accepted (revision defaults to 0).

export const parseVersion = (version) => {
    if (typeof version !== 'string') return null;
    const match = version.trim().match(/^(\d+)\.(\d+)\.(\d+)(?:\.(\d+))?$/);
    if (!match) return null;
    return [Number(match[1]), Number(match[2]), Number(match[3]), Number(match[4] ?? 0)];
}

export const compareVersions = (a, b) => {
    const pa = parseVersion(a);
    const pb = parseVersion(b);
    if (!pa || !pb) return null;
    for (let i = 0; i < 4; i++) {
        if (pa[i] !== pb[i]) return pa[i] < pb[i] ? -1 : 1;
    }
    return 0;
}

export const isVersionAtLeast = (version, target) => {
    const cmp = compareVersions(version, target);
    return cmp !== null && cmp >= 0;
}

export const isVersionBelow = (version, target) => {
    const cmp = compareVersions(version, target);
    return cmp !== null && cmp < 0;
}
