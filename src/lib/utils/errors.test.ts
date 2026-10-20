import { describe, expect, it } from 'vitest';
import { ErrorCause } from './errors';

describe('ErrorCause', () => {
    it('recognises a system error in the language of the system', () => {
        // Windows words its errors in the system's language: the number is what is left
        expect(ErrorCause('Could not write C:\\x: Accès refusé. (os error 5)', true)).toBe('errors.access_denied');
        expect(ErrorCause('Failed to write file: Espace insuffisant sur le disque. (os error 112)', true)).toBe('errors.disk_full');
        expect(ErrorCause('Permission denied (os error 13)', false)).toBe('errors.access_denied');
        expect(ErrorCause('No space left on device (os error 28)', false)).toBe('errors.disk_full');
    });

    it('reads an error number as the system it came from means it', () => {
        // 5 is an input/output error on Linux, not a refusal
        expect(ErrorCause('Input/output error (os error 5)', false)).toBeNull();
        expect(ErrorCause('Zugriff verweigert (os error 13)', true)).toBeNull();
    });

    it('knows network and server failures by their words', () => {
        expect(ErrorCause(new Error('HTTP error status: 403'))).toBe('errors.rate_limited');
        expect(ErrorCause('Request to https://x failed: error sending request')).toBe('errors.offline');
        expect(ErrorCause('something else')).toBeNull();
    });
});
