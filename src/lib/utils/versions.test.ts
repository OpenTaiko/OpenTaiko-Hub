import { describe, expect, it } from 'vitest';
import { compareVersions, gameUpdateNeedsSkins, isSkinCompatible, isVersionAtLeast, isVersionBelow, isVersionInSeries, parseVersion, skinFit } from './versions';

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

    it('takes skins to fit when only the patch number differs', () => {
        expect(isSkinCompatible('0.6.0.1', '0.6.0.83')).toBe(true);
        expect(isSkinCompatible('0.6.0.83', '0.6.1.0')).toBe(false);
        expect(isSkinCompatible('0.6.0', '0.6.0.1')).toBe(false);
        expect(isSkinCompatible('v0.6.0.1', '0.6.0.1')).toBe(false);
        expect(isSkinCompatible(null, '0.6.0.1')).toBe(false);
        expect(isSkinCompatible('Unknown', 'Unknown')).toBe(false);
    });

    it('takes skins from 0.7.0.0 on to fit the latest game', () => {
        // skin, game, latest game, the instance already has this skin version
        expect(skinFit('0.7.2.0', '0.7.0.2', '0.7.0.2', false)).toBe('fits');
        expect(skinFit('0.7.2.0', '0.7.0.2', '0.7.3.0', true)).toBe('fits');
        expect(skinFit('0.7.2.0', '0.7.0.2', '0.7.3.0', false)).toBe('update_game');
        expect(skinFit('0.7.0.0', '0.8.0.0', '0.8.0.0', false)).toBe('fits');
        // Latest release unknown (offline): nothing to warn about
        expect(skinFit('0.7.2.0', '0.7.0.2', null, false)).toBe('fits');
        // Before 0.7.0.0 on either side, the first three numbers must match
        expect(skinFit('0.7.0.1', '0.6.1.4', '0.7.0.2', false)).toBe('incompatible');
        expect(skinFit('0.6.1.3', '0.6.1.0', '0.7.0.2', false)).toBe('fits');
        expect(skinFit('0.6.0.9', '0.7.0.2', '0.7.0.2', true)).toBe('incompatible');
        expect(skinFit('0.7.0.0', null, '0.7.0.2', false)).toBe('incompatible');
    });

    it('asks for new skins on a game update only before 0.7.0.0', () => {
        expect(gameUpdateNeedsSkins('0.7.0.2', '0.7.1.0')).toBe(false);
        expect(gameUpdateNeedsSkins('0.7.0.0', '0.8.0.0')).toBe(false);
        expect(gameUpdateNeedsSkins('0.6.1.4', '0.7.0.2')).toBe(true);
        expect(gameUpdateNeedsSkins('0.6.0.83', '0.6.1.0')).toBe(true);
        expect(gameUpdateNeedsSkins('0.6.1.2', '0.6.1.4')).toBe(false);
        expect(gameUpdateNeedsSkins(null, '0.7.0.2')).toBe(true);
    });
});
