import { describe, expect, it } from 'vitest';
import { IsPlainRelPath } from './path';

describe('IsPlainRelPath', () => {
    it('accepts plain relative catalog paths', () => {
        expect(IsPlainRelPath('01 Pop\\Song\\song.tja')).toBe(true);
        expect(IsPlainRelPath('Characters/Name/a.png')).toBe(true);
        expect(IsPlainRelPath('Re..Zero')).toBe(true);
    });

    it('refuses anything that could place a file elsewhere', () => {
        expect(IsPlainRelPath('')).toBe(false);
        expect(IsPlainRelPath('..\\..\\Windows\\x.dll')).toBe(false);
        expect(IsPlainRelPath('Pop/../../x')).toBe(false);
        expect(IsPlainRelPath('C:\\Windows\\x.dll')).toBe(false);
        expect(IsPlainRelPath('/etc/passwd')).toBe(false);
        expect(IsPlainRelPath('\\\\server\\share')).toBe(false);
    });
});
