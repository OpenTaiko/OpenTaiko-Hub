import { describe, expect, it, vi } from 'vitest';
import { ValidSoundtrack, ValidAssetCatalog, ValidRelease, ValidReleaseList } from './validate';

const goodSong = {
    uniqueId: 'a', tjaFolderPath: 'Pop\\A', tjaFilesPath: ['Pop\\A\\a.tja'], tjaGenreFolder: 'Pop', tjaMD5: 'x',
    chartSize: 2.5, chartTitle: 'A', chartDifficulties: { Oni: 8 }, chartMakers: {}, chartAudioFilePath: 'a.ogg',
    someFieldAddedLater: true
};

describe('ValidSoundtrack', () => {
    it('keeps valid entries, including fields it does not know', () => {
        expect(ValidSoundtrack([goodSong])).toEqual([goodSong]);
    });

    it('drops an entry missing a field the Hub relies on', () => {
        vi.spyOn(console, 'warn').mockImplementation(() => {});
        const { tjaFolderPath: _, ...noFolder } = goodSong;
        const badFiles = { ...goodSong, uniqueId: 'b', tjaFilesPath: ['ok', 3] };
        expect(ValidSoundtrack([goodSong, noFolder, badFiles, null, 'text']).map((s) => s.uniqueId)).toEqual(['a']);
    });

    it('refuses something that is not a list', () => {
        expect(() => ValidSoundtrack({ songs: [] })).toThrow();
    });
});

describe('ValidAssetCatalog', () => {
    const skin = { skinName: 'S', skinFolder: 'S', skinVersion: '0.6.0', skinSize: 10, skinFilesPath: [] };
    const chara = { charaName: 'C', charaFolder: 'C', charaVersion: '0.6.0', charaSize: 1, charaFilesPath: ['Characters\\C\\a.png'] };

    it('checks each list with its own field prefix', () => {
        vi.spyOn(console, 'warn').mockImplementation(() => {});
        const catalog = ValidAssetCatalog({ Skins: [skin, chara], Characters: [chara, skin], Puchicharas: [chara] });
        expect(catalog.Skins).toEqual([skin]);
        expect(catalog.Characters).toEqual([chara]);
        expect(catalog.Puchicharas).toEqual([chara]);
    });

    it('gives an empty list for a missing type', () => {
        expect(ValidAssetCatalog({ Skins: [skin] }).Characters).toEqual([]);
    });
});

describe('ValidRelease', () => {
    it('keeps a release and drops malformed assets', () => {
        vi.spyOn(console, 'warn').mockImplementation(() => {});
        const release = ValidRelease({
            tag_name: '0.6.0.83',
            prerelease: false,
            assets: [
                { name: 'OpenTaiko.Win.x64.zip', browser_download_url: 'https://github.com/x/y.zip' },
                { name: 'broken' }
            ]
        });
        expect(release.tag_name).toBe('0.6.0.83');
        expect(release.draft).toBe(false);
        expect(release.assets.map((a) => a.name)).toEqual(['OpenTaiko.Win.x64.zip']);
    });

    it('refuses an answer that is not a release', () => {
        expect(() => ValidRelease({ message: 'API rate limit exceeded' })).toThrow();
        expect(() => ValidRelease(null)).toThrow();
        expect(ValidReleaseList([{ tag_name: 'a', assets: [] }, { message: 'x' }]).map((r) => r.tag_name)).toEqual(['a']);
        expect(() => ValidReleaseList({ message: 'x' })).toThrow();
    });
});
