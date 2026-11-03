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

/**
 * True when skins made for one version also fit the other: both are full four-part
 * versions (<main>.<major>.<minor>.<patch>) with the same first three parts. Anything
 * else, an unknown version included, counts as not compatible.
 */
export const isSkinCompatible = (a: string | null | undefined, b: string | null | undefined): boolean => {
    const full = /^\d+\.\d+\.\d+\.\d+$/;
    if (!a || !b || !full.test(a) || !full.test(b)) return false;
    return a.split('.').slice(0, 3).join('.') === b.split('.').slice(0, 3).join('.');
};

/** From this version on, skins and the latest game always work together: an API call
 *  the game deprecates stays usable. */
export const SKIN_API_STABLE = '0.7.0.0';

/**
 * True when updating the game from `from` to `to` leaves the installed skins behind: they
 * must then be updated too. From 0.7.0.0 to a later version they keep working. An unknown
 * version on either side counts as needing it, as isSkinCompatible does.
 */
export const gameUpdateNeedsSkins = (from: string | null | undefined, to: string | null | undefined): boolean =>
    !(isVersionAtLeast(from, SKIN_API_STABLE) && isVersionAtLeast(to, SKIN_API_STABLE)) && !isSkinCompatible(to, from);

export type SkinFit = 'fits' | 'update_game' | 'incompatible';

/**
 * How a skin version fits the game of an instance. Before 0.7.0.0 they must share their
 * first three numbers (isSkinCompatible). From 0.7.0.0 on both, they always fit; a skin
 * version the instance does not have yet may need the latest game, so it is flagged while
 * the game is behind the latest release (an unknown latest release flags nothing).
 */
export const skinFit = (
    skin: string | null | undefined,
    game: string | null | undefined,
    latestGame: string | null | undefined,
    hasThisVersion: boolean
): SkinFit => {
    if (isVersionAtLeast(skin, SKIN_API_STABLE) && isVersionAtLeast(game, SKIN_API_STABLE)) {
        return !hasThisVersion && !!latestGame && isVersionBelow(game, latestGame) ? 'update_game' : 'fits';
    }
    return isSkinCompatible(game, skin) ? 'fits' : 'incompatible';
};
