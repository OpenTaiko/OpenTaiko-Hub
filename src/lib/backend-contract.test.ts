import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { isCancelled } from './stores/activity';

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
        // instances.ts looks for this word to explain the failure
        expect(rust('configini.rs')).toMatch(/cannot be stored in Config\.ini \(Shift_JIS\)/);
        expect(readFileSync(new URL('./stores/instances.ts', import.meta.url), 'utf8')).toContain("includes('Shift_JIS')");
    });
});
