import { describe, expect, it } from 'vitest';
import { VaultSlots, buildLibraryTree, type LibraryFolder } from './library';
import type { SoundtrackEntry } from '$lib/types';

const chart = (uniqueId: string, folder: string, levels: Record<string, number>): SoundtrackEntry => ({
    uniqueId, tjaFolderPath: folder, tjaFilesPath: [], tjaGenreFolder: folder.split('\\')[0], tjaMD5: '',
    chartSize: 1, chartTitle: uniqueId, chartTitles: {}, chartSubtitle: '', chartSubtitles: {},
    chartDifficulties: levels, chartMakers: {}, chartAudioFilePath: ''
} as unknown as SoundtrackEntry);

const entriesOf = (root: LibraryFolder): LibraryFolder['entries'] =>
    [...root.entries, ...[...root.children.values()].flatMap(entriesOf)];

describe('vault charts', () => {
    it('show two charts under EX and EXEX, and a lone one under EXEX', () => {
        expect(VaultSlots({ Oni: 3, Edit: 5 })).toEqual([{ course: 'Oni', level: 3 }, { course: 'Edit', level: 5 }]);
        expect(VaultSlots({ Edit: 2.5 })).toEqual([null, { course: 'Edit', level: 2.5 }]);
        expect(VaultSlots({ Oni: 1 })).toEqual([null, { course: 'Oni', level: 1 }]);
        // The game shows the vault's EX and EXEX charts only
        expect(VaultSlots({ Easy: 1, Hard: 4, Edit: 0 })).toEqual([null, { course: 'Edit', level: 0 }]);
        expect(VaultSlots({})).toEqual([null, null]);
    });

    it('are the charts of the Secret Vault genre', () => {
        const catalog = [
            chart('vault007', 'S3 Secret Vault\\X007 - Vaulterbomb', { Edit: 5, Oni: 3 }),
            chart('citys', '01 OpenTaiko Chapter I\\001 - CITYSCAPE', { Oni: 6, Edit: 9 })
        ];
        const root = buildLibraryTree({
            scanned: [], genres: {}, catalog, catalogById: new Map(catalog.map((c) => [c.uniqueId, c])),
            installed: {}, labelFor: (_entry, fallback) => fallback
        });
        const vault = Object.fromEntries(entriesOf(root).map((e) => [e.key, e.vault]));
        expect(vault).toEqual({ vault007: true, citys: false });
    });
});
