import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { GAME_REPO, HUB_REPO, SKINS_REPO, SOUNDTRACK_REPO, apiUrl, pageUrl, rawUrl, sourceZipUrl } from './repos';

describe('repositories', () => {
    it('are exactly the ones the backend downloads from', () => {
        // REPOSITORIES in download.rs: one missing there makes every download from it
        // fail, and one there that the Hub does not use is trusted for nothing
        const source = readFileSync(new URL('../../src-tauri/src/download.rs', import.meta.url), 'utf8');
        const list = source.match(/const REPOSITORIES: \[&str; \d+\] = \[([^\]]*)\]/)?.[1] ?? '';
        const allowed = [...list.matchAll(/"([^"]+)"/g)].map((m) => m[1]).sort();
        const used = [GAME_REPO, HUB_REPO, SKINS_REPO, SOUNDTRACK_REPO].map((r) => `${r.owner}/${r.name}`.toLowerCase()).sort();
        expect(allowed).toEqual(used);
    });

    it('build the addresses the Hub used before', () => {
        expect(rawUrl(SOUNDTRACK_REPO, 'soundtrack_info.json')).toBe('https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/soundtrack_info.json');
        expect(apiUrl(GAME_REPO)).toBe('https://api.github.com/repos/0AuBSQ/OpenTaiko');
        expect(pageUrl(HUB_REPO, 'releases/latest')).toBe('https://github.com/OpenTaiko/OpenTaiko-Hub/releases/latest');
        expect(sourceZipUrl(GAME_REPO, 'abc')).toBe('https://codeload.github.com/0AuBSQ/OpenTaiko/zip/abc');
    });
});
