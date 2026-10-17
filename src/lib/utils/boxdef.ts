// box.def, as a document rather than a blob of text.
//
// A real box.def carries far more than the Hub shows a form for — #BOXTYPE, #BGTYPE,
// #SCENEPRESET, per-language titles and explanations, colours, #BOXCHARA. Editing one
// field must never drop the rest, so the file is kept as its original lines and a field
// edit rewrites only the line it owns, appending when the directive was not there.

/** One physical line: a directive when it parsed as one, otherwise kept verbatim. */
interface Line {
    raw: string;
    key: string | null;
    value: string;
}

export interface BoxDefDoc {
    lines: Line[];
    /** The line ending the file already used, so saving does not rewrite every line. */
    eol: string;
    /** True when the file had a UTF-8 BOM, which is preserved. */
    bom: boolean;
}

/**
 * Language suffixes the game accepts on #TITLE and #BOXEXPLANATION. The code alone is
 * ambiguous (GB and EN are both English, NL is Dutch not "Netherlandish"), so each
 * carries the language it actually means.
 */
export const BOX_LANGS = [
    { code: 'EN', name: 'English' },
    { code: 'GB', name: 'English (UK)' },
    { code: 'JA', name: '日本語' },
    { code: 'FR', name: 'Français' },
    { code: 'ES', name: 'Español' },
    { code: 'DE', name: 'Deutsch' },
    { code: 'NL', name: 'Nederlands' },
    { code: 'ZH', name: '中文' },
    { code: 'KO', name: '한국어' },
    { code: 'RU', name: 'Русский' }
] as const;

export type BoxLang = (typeof BOX_LANGS)[number]['code'];

export const parseBoxDef = (text: string): BoxDefDoc => {
    const bom = text.startsWith('﻿');
    const body = bom ? text.slice(1) : text;
    const eol = body.includes('\r\n') ? '\r\n' : '\n';

    const lines: Line[] = body.split(/\r\n|\n/).map((raw) => {
        const trimmed = raw.trim();
        if (!trimmed.startsWith('#')) return { raw, key: null, value: '' };
        const colon = trimmed.indexOf(':');
        if (colon < 0) return { raw, key: null, value: '' };
        return {
            raw,
            key: trimmed.slice(0, colon).toUpperCase(),
            value: trimmed.slice(colon + 1).trim()
        };
    });

    return { lines, eol, bom };
};

export const getField = (doc: BoxDefDoc, key: string): string => {
    const upper = key.toUpperCase();
    return doc.lines.find((l) => l.key === upper)?.value ?? '';
};

/**
 * Sets a directive, or removes it when `value` is empty. Returns a new document; the
 * lines it does not own are carried over untouched.
 */
export const setField = (doc: BoxDefDoc, key: string, value: string): BoxDefDoc => {
    const upper = key.toUpperCase();
    const clean = value.trim();
    const at = doc.lines.findIndex((l) => l.key === upper);

    if (at < 0) {
        if (!clean) return doc;
        return { ...doc, lines: [...doc.lines, { raw: `${upper}:${clean}`, key: upper, value: clean }] };
    }

    const lines = [...doc.lines];
    if (!clean) {
        lines.splice(at, 1);
    } else {
        lines[at] = { raw: `${upper}:${clean}`, key: upper, value: clean };
    }
    return { ...doc, lines };
};

export const serializeBoxDef = (doc: BoxDefDoc): string => {
    // A file that ended with a newline keeps ending with one: the parser turns that into
    // a trailing empty line, so joining restores it exactly.
    const body = doc.lines.map((l) => l.raw).join(doc.eol);
    return (doc.bom ? '﻿' : '') + body;
};

/** Every directive in the file, for the "everything else" list. */
export const directivesOf = (doc: BoxDefDoc): { key: string; value: string }[] =>
    doc.lines.filter((l): l is Line & { key: string } => l.key !== null)
        .map((l) => ({ key: l.key, value: l.value }));

/** A colour the game accepts, or null when the value is not one. */
export const asColor = (value: string): string | null =>
    /^#[0-9a-fA-F]{6}$/.test(value.trim()) ? value.trim().toLowerCase() : null;
