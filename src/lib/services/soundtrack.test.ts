import { describe, expect, it } from 'vitest';
import { SongInstallEntries, SongGenrePaths, SoundtrackFileUrl } from './soundtrack';
import type { SoundtrackEntry } from '$lib/types';

const song = (tjaFolderPath: string, tjaFilesPath: string[]): SoundtrackEntry => ({
    uniqueId: 'id', tjaFolderPath, tjaFilesPath, tjaGenreFolder: '', tjaMD5: '', chartSize: 1, chartTitle: '',
    chartDifficulties: {}, chartMakers: {}, chartAudioFilePath: ''
});

describe('SongInstallEntries', () => {
    it('places files relative to the song folder, with forward slashes', () => {
        const entries = SongInstallEntries(song('01 Pop\\Song', ['01 Pop\\Song\\song.tja', '01 Pop\\Song\\sub\\a.ogg']), null);
        expect(entries.map((e) => e.relPath)).toEqual(['song.tja', 'sub/a.ogg']);
        expect(entries[0].url).toBe('https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/01 Pop/Song/song.tja');
        expect(entries[0].gitSha).toBeNull();
    });

    it('keeps only the name of a file listed outside the song folder', () => {
        const entries = SongInstallEntries(song('01 Pop\\Song', ['02 Other\\Elsewhere\\x.ogg']), null);
        expect(entries[0].relPath).toBe('x.ogg');
    });

    it('does not treat a sibling folder sharing the prefix as the song folder', () => {
        const entries = SongInstallEntries(song('01 Pop\\Song', ['01 Pop\\Song2\\x.ogg']), null);
        expect(entries[0].relPath).toBe('x.ogg');
    });

    it('carries the git blob SHA of each file when the index is known', () => {
        const shaMap = new Map([['01 Pop/Song/song.tja', 'abc']]);
        const entries = SongInstallEntries(song('01 Pop\\Song', ['01 Pop\\Song\\song.tja', '01 Pop\\Song\\song.ogg']), shaMap);
        expect(entries.map((e) => e.gitSha)).toEqual(['abc', null]);
    });
});

describe('SongGenrePaths', () => {
    it('lists every genre folder above the song, outermost first', () => {
        expect(SongGenrePaths(song('A\\B\\Song', []))).toEqual(['A', 'A/B']);
        expect(SongGenrePaths(song('Song', []))).toEqual([]);
    });
});

describe('SoundtrackFileUrl', () => {
    it('turns repository backslashes into URL slashes', () => {
        expect(SoundtrackFileUrl('A\\B\\box.def')).toBe('https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/A/B/box.def');
    });
});
