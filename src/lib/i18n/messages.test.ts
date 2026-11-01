import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { IntlMessageFormat } from 'intl-messageformat';
import { TYPE, type MessageFormatElement } from '@formatjs/icu-messageformat-parser';

// Every message of every language is formatted the way the Hub shows it. A placeholder
// that survives as text means the user reads "{name}" instead of a name: Korean quoted
// its names with ASCII apostrophes, which ICU takes as an escape, so the delete
// confirmation never said what it was about to delete.

const dir = new URL('.', import.meta.url);
const locales = readdirSync(dir).filter((file) => file.endsWith('.json'));

const flatten = (node: unknown, prefix = ''): [string, string][] =>
    Object.entries(node as Record<string, unknown>).flatMap(([key, value]) =>
        typeof value === 'string' ? [[prefix + key, value] as [string, string]] : flatten(value, `${prefix}${key}.`)
    );

type Values = Record<string, unknown>;

// A value for every argument, of the kind it is used as
const valuesFor = (elements: MessageFormatElement[], values: Values = {}): Values => {
    for (const element of elements) {
        switch (element.type) {
            case TYPE.argument:
                values[element.value] ??= 'X';
                break;
            case TYPE.number:
                values[element.value] = 2;
                break;
            case TYPE.date:
            case TYPE.time:
                values[element.value] = new Date(0);
                break;
            case TYPE.plural:
                values[element.value] = 2;
                for (const option of Object.values(element.options)) valuesFor(option.value, values);
                break;
            case TYPE.select:
                values[element.value] = 'other';
                for (const option of Object.values(element.options)) valuesFor(option.value, values);
                break;
            case TYPE.tag:
                values[element.value] = (chunks: unknown[]) => chunks.join('');
                valuesFor(element.children, values);
                break;
        }
    }
    return values;
};

describe.each(locales)('%s', (file) => {
    const locale = file.replace(/\.json$/, '');
    const messages = flatten(JSON.parse(readFileSync(new URL(file, dir), 'utf8')));

    it('formats every message without a leftover placeholder', () => {
        const broken: string[] = [];
        for (const [key, message] of messages) {
            try {
                // As svelte-i18n formats them: markup such as <br> is text, not ICU tags
                const format = new IntlMessageFormat(message, locale, undefined, { ignoreTag: true });
                const text = String(format.format(valuesFor(format.getAst()) as Record<string, never>));
                if (/[{}]/.test(text)) broken.push(`${key}: ${text}`);
            } catch (error) {
                broken.push(`${key}: ${String(error)}`);
            }
        }
        expect(broken).toEqual([]);
    });
});
