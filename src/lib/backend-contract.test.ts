import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { isCancelled } from './stores/activity';
import { IsSongsPathUnstorable } from './utils/errors';

// The backend's errors are plain text, and the page recognises two of them by their
// words. A change of wording on one side would go unnoticed until the page misread a
// cancelled job as a failure, or stopped explaining a Config.ini it cannot write.
const rust = (file: string) => readFileSync(new URL(`../../src-tauri/src/${file}`, import.meta.url), 'utf8');

describe('error texts the page relies on', () => {
    it('a cancelled job', () => {
        const constant = rust('jobs.rs').match(/pub const CANCELLED: &str = "([^"]+)";/)?.[1];
        expect(constant).toBeDefined();
        expect(isCancelled(constant)).toBe(true);
        expect(isCancelled(new Error(constant))).toBe(true);
    });

    it('a library path Config.ini cannot hold', () => {
        // Every message the backend words for it, filled in as the backend would
        const messages = [...rust('configini.rs').matchAll(/"([^"]*cannot be stored in Config\.ini[^"]*)"/g)].map((m) => m[1]);
        expect(messages.length).toBeGreaterThanOrEqual(2);
        for (const message of messages) {
            expect(IsSongsPathUnstorable(message.replace('{}', 'D:\\Généreux\\Songs'))).toBe(true);
            expect(IsSongsPathUnstorable(new Error(message.replace('{}', '/home/rené/Songs')))).toBe(true);
        }
        // Other failures to link the library are not explained as this one
        expect(IsSongsPathUnstorable('Instance folder not found: D:\\Games\\OpenTaiko')).toBe(false);
    });
});
