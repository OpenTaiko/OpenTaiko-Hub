import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';

// ipc.ts is written by hand. A command renamed or removed in the backend, or an argument
// renamed on one side only, used to be found only when the page called it; this finds it
// when the tests run.
const ipc = readFileSync(new URL('./ipc.ts', import.meta.url), 'utf8');
const rustDir = new URL('../../src-tauri/src/', import.meta.url);
const rust = readdirSync(rustDir)
    .filter((name) => name.endsWith('.rs'))
    .map((name) => readFileSync(new URL(name, rustDir), 'utf8'))
    .join('\n');

/** The text between the bracket at `open` and the one closing it. */
const enclosed = (text: string, open: number): string => {
    const [start, end] = [text[open], { '[': ']', '(': ')', '{': '}' }[text[open]]];
    let depth = 0;
    for (let i = open; i < text.length; i++) {
        if (text[i] === start) depth++;
        else if (text[i] === end && --depth === 0) return text.slice(open + 1, i);
    }
    throw new Error(`Unclosed ${start} at ${open}`);
};

/** Splits on commas that are not inside <>, (), [] or {}. */
const splitTop = (text: string): string[] => {
    const parts: string[] = [];
    let depth = 0;
    let current = '';
    for (const ch of text) {
        if ('<([{'.includes(ch)) depth++;
        if ('>)]}'.includes(ch)) depth--;
        if (ch === ',' && depth === 0) {
            parts.push(current);
            current = '';
        } else {
            current += ch;
        }
    }
    parts.push(current);
    return parts.map((p) => p.trim()).filter(Boolean);
};

const camel = (snake: string) => snake.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());

// Arguments Tauri fills in itself rather than the page
const FRAMEWORK = /^(?:tauri::)?(?:AppHandle|Window|Webview|WebviewWindow|State|ipc::Request|Request)\b/;

interface Param { name: string; optional: boolean; framework: boolean }

/** Every command's parameters, as the page names them. */
const commands = new Map<string, Param[]>();
for (const match of rust.matchAll(/#\[tauri::command\]\s*(?:pub(?:\(crate\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*(?:<[^(]*>)?\s*\(/g)) {
    const params = splitTop(enclosed(rust, match.index! + match[0].length - 1)).map((param) => {
        const [name, ...type] = param.split(':');
        const typeText = type.join(':').trim();
        return { name: camel(name.trim().replace(/^mut\s+/, '')), optional: typeText.startsWith('Option<'), framework: FRAMEWORK.test(typeText) };
    });
    commands.set(match[1], params);
}

interface Call { command: string; keys: string[] | null }

/** Every invoke in ipc.ts, with the argument names it passes (null for a raw body). */
const calls: Call[] = [...ipc.matchAll(/\binvoke(?:<[^(]*>)?\(/g)].map((match) => {
    const args = splitTop(enclosed(ipc, match.index! + match[0].length - 1));
    const command = args[0]?.match(/^'([a-z_]+)'$/)?.[1] ?? `not a literal: ${args[0]}`;
    if (args.length < 2) return { command, keys: [] };
    if (!args[1].startsWith('{')) return { command, keys: null };
    return { command, keys: splitTop(args[1].slice(1, -1)).map((entry) => entry.split(':')[0].trim()) };
});

describe('ipc.ts', () => {
    it('finds the commands and the calls', () => {
        // A call whose name is not a literal is kept, under a name no command has, and
        // fails below instead of being skipped
        expect(commands.size).toBeGreaterThan(40);
        expect(calls.length).toBeGreaterThan(40);
    });

    it('calls only commands the backend registers', () => {
        const main = readFileSync(new URL('main.rs', rustDir), 'utf8');
        const start = main.indexOf('generate_handler![');
        const registered = new Set(
            splitTop(enclosed(main, start + 'generate_handler!'.length)).map((item) => item.split('::').pop() ?? '')
        );
        expect(registered.size).toBeGreaterThan(40);
        expect(calls.filter((call) => !registered.has(call.command)).map((call) => call.command)).toEqual([]);
        expect(calls.filter((call) => !commands.has(call.command)).map((call) => call.command)).toEqual([]);
    });

    it('passes the arguments each command takes, under their names', () => {
        const problems: string[] = [];
        for (const { command, keys } of calls) {
            const params = commands.get(command);
            if (!params || keys === null) continue;
            const page = params.filter((p) => !p.framework);
            for (const key of keys) {
                if (!page.some((p) => p.name === key)) problems.push(`${command}: no parameter ${key}`);
            }
            for (const param of page) {
                if (!param.optional && !keys.includes(param.name)) problems.push(`${command}: ${param.name} not passed`);
            }
        }
        expect(problems).toEqual([]);
    });

    it('sends a raw body only to a command that reads the request itself', () => {
        const raw = calls.filter((call) => call.keys === null).map((call) => call.command);
        expect(raw).toEqual(['write_saves']);
        expect(commands.get('write_saves')?.some((p) => p.framework && p.name === 'request')).toBe(true);
    });
});
