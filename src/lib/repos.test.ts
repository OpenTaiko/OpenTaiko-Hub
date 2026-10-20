import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { GAME_REPO, HUB_REPO, SKINS_REPO, SOUNDTRACK_REPO, apiUrl, pageUrl, rawUrl, sourceZipUrl } from './repos';

describe('repositories', () => {
    it('are all accounts the backend downloads from', () => {
        // OWNERS in download.rs: an account missing there makes every download from it fail
        const source = readFileSync(new URL('../../src-tauri/src/download.rs', import.meta.url), 'utf8');
        const owners = source.match(/const OWNERS: \[&str; \d+\] = \[([^\]]*)\]/)?.[1] ?? '';
        for (const repo of [GAME_REPO, HUB_REPO, SKINS_REPO, SOUNDTRACK_REPO]) {
            expect(owners).toContain(`"${repo.owner.toLowerCase()}"`);
        }
    });

    it('build the addresses the Hub used before', () => {
        expect(rawUrl(SOUNDTRACK_REPO, 'soundtrack_info.json')).toBe('https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/soundtrack_info.json');
        expect(apiUrl(GAME_REPO)).toBe('https://api.github.com/repos/0AuBSQ/OpenTaiko');
        expect(pageUrl(HUB_REPO, 'releases/latest')).toBe('https://github.com/OpenTaiko/OpenTaiko-Hub/releases/latest');
        expect(sourceZipUrl(GAME_REPO, 'abc')).toBe('https://codeload.github.com/0AuBSQ/OpenTaiko/zip/abc');
    });
});
