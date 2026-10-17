import { describe, expect, it } from 'vitest';
import { resolveInstances } from './instances';
import type { Instance } from '$lib/types';

const instance = (id: string, path: string): Instance => ({
    id, name: id, path, channel: 'stable', experimental: null, createdAt: '2026-01-01T00:00:00Z'
});

const resolve = (storedList: Instance[], storedActiveId: string | null, legacyPath: string | null, onDisk: string[]) =>
    resolveInstances({
        storedList,
        storedActiveId,
        legacyPath,
        pathExists: async (p) => onDisk.includes(p),
        makeInstance: (path) => instance('adopted', path)
    });

describe('resolveInstances', () => {
    it('adopts the legacy game folder of an upgraded install and selects it', async () => {
        const result = await resolve([], null, 'C:\\Hub\\OpenTaiko', ['C:\\Hub\\OpenTaiko']);
        expect(result.adopted).toBe(true);
        expect(result.list.map((i) => i.path)).toEqual(['C:\\Hub\\OpenTaiko']);
        expect(result.activeId).toBe('adopted');
    });

    it('adopts it even when a registry from another Hub build already exists', async () => {
        const other = instance('other', 'D:\\Games\\OpenTaiko');
        const result = await resolve([other], 'other', 'C:\\Hub\\OpenTaiko', ['C:\\Hub\\OpenTaiko', 'D:\\Games\\OpenTaiko']);
        expect(result.list.map((i) => i.id)).toEqual(['other', 'adopted']);
        expect(result.activeId).toBe('adopted');
    });

    it('does not adopt a folder already registered, whatever its spelling on Windows', async () => {
        const known = instance('known', 'c:/hub/opentaiko/');
        const result = await resolve([known], 'known', 'C:\\Hub\\OpenTaiko', ['C:\\Hub\\OpenTaiko']);
        expect(result.adopted).toBe(false);
        expect(result.list).toHaveLength(1);
    });

    it('keeps the stored selection, or falls back to an instance that still exists', async () => {
        const a = instance('a', '/games/a');
        const b = instance('b', '/games/b');
        expect((await resolve([a, b], 'b', null, ['/games/a', '/games/b'])).activeId).toBe('b');
        expect((await resolve([a, b], 'gone', null, ['/games/b'])).activeId).toBe('b');
        expect((await resolve([], null, null, [])).activeId).toBeNull();
    });
});
