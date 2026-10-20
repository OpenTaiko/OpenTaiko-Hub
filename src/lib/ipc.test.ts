import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

// ipc.ts is written by hand. A command renamed or removed in the backend used to be
// found only when the page called it; this finds it when the tests run.
describe('ipc.ts', () => {
    it('calls only commands the backend registers', () => {
        const ipc = readFileSync(new URL('./ipc.ts', import.meta.url), 'utf8');
        const main = readFileSync(new URL('../../src-tauri/src/main.rs', import.meta.url), 'utf8');
        const handler = main.match(/generate_handler!\[([\s\S]*?)\]/)?.[1] ?? '';
        const registered = new Set(
            handler.split(',').map((item) => item.trim().split('::').pop() ?? '').filter(Boolean)
        );
        const called = [...ipc.matchAll(/invoke(?:<[^(]*>)?\('([a-z_]+)'/g)].map((m) => m[1]);
        expect(called.length).toBeGreaterThan(30);
        expect(called.filter((name) => !registered.has(name))).toEqual([]);
    });
});
