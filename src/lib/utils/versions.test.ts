import { describe, expect, it } from 'vitest';
import { compareVersions, isVersionAtLeast, isVersionBelow, isVersionInSeries, parseVersion } from './versions';

describe('versions', () => {
    it('parses three and four part versions', () => {
        expect(parseVersion('0.6.0')).toEqual([0, 6, 0, 0]);
        expect(parseVersion(' 0.6.0.107 ')).toEqual([0, 6, 0, 107]);
        expect(parseVersion('v0.6.0')).toBeNull();
        expect(parseVersion(null)).toBeNull();
    });

    it('compares part by part, numerically', () => {
        expect(compareVersions('0.6.0.10', '0.6.0.9')).toBe(1);
        expect(compareVersions('0.6.0', '0.6.0.0')).toBe(0);
        expect(compareVersions('0.5.9', '0.6.0')).toBe(-1);
        expect(compareVersions('nope', '0.6.0')).toBeNull();
    });

    it('answers unknown versions with false rather than guessing', () => {
        expect(isVersionAtLeast('0.6.1', '0.6.1')).toBe(true);
        expect(isVersionAtLeast(null, '0.6.1')).toBe(false);
        expect(isVersionBelow('0.6.0.107', '0.6.1')).toBe(true);
        expect(isVersionBelow(undefined, '0.6.1')).toBe(false);
    });

    it('matches a release series whatever the revision', () => {
        expect(isVersionInSeries('0.6.0.107', 0, 6, 0)).toBe(true);
        expect(isVersionInSeries('0.7.0', 0, 6, 0)).toBe(false);
    });
});
