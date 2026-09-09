// Shared sql.js loader. initSqlJs compiles a fresh ~1.5MB WASM module on every call and
// the module is never freed, so calling it per-component / per-operation leaks memory and
// eventually crashes the WebView renderer. Initialise it exactly once for the whole app.
import initSqlJs, { type SqlJsStatic } from 'sql.js';
import sqlWasmUrl from 'sql.js/dist/sql-wasm.wasm?url';

let promise: Promise<SqlJsStatic> | null = null;

export const getSQL = (): Promise<SqlJsStatic> => {
    if (!promise) {
        promise = initSqlJs({ locateFile: () => sqlWasmUrl }).catch((error: unknown) => {
            // Do not cache the failure: a transient error would otherwise poison every
            // later call for the rest of the session.
            promise = null;
            throw error;
        });
    }
    return promise;
};
