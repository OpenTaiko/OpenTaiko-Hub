import { describe, expect, it } from 'vitest';
import { FromLegacyFiles } from './settings';

describe('settings', () => {
    it('take over what the three files of older Hub versions held', () => {
        expect(FromLegacyFiles({ theme: 'seafoam' }, { thememode: 'light' }, { locale: 'fr' }))
            .toEqual({ theme: 'seafoam', mode: 'light', locale: 'fr' });
        // Missing or damaged files leave that setting at its default
        expect(FromLegacyFiles(null, { thememode: 3 }, { locale: 'ja' }))
            .toEqual({ theme: undefined, mode: undefined, locale: 'ja' });
    });
});
