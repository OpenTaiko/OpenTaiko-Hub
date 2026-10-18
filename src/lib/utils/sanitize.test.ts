import { describe, expect, it } from 'vitest';
import { SafeUrl } from './sanitize';

describe('SafeUrl', () => {
    it('keeps plain https links', () => {
        expect(SafeUrl('https://www.youtube.com/watch?v=abc')).toBe('https://www.youtube.com/watch?v=abc');
        expect(SafeUrl('  https://example.com/artist  ')).toBe('https://example.com/artist');
    });

    it('drops anything that is not https', () => {
        for (const url of [
            'javascript:alert(1)',
            ' JavaScript:alert(1)',
            'data:text/html,<script>alert(1)</script>',
            'http://example.com',
            'file:///C:/Windows/notepad.exe',
            '//example.com',
            'example.com',
            '',
            null,
            undefined
        ]) {
            expect(SafeUrl(url)).toBeNull();
        }
    });
});
