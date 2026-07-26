// Shared sql.js loader. initSqlJs compiles a fresh ~1.5MB WASM module on every call and
// the module is never freed, so calling it per-component / per-operation leaks memory and
// eventually crashes the WebView renderer. Initialise it exactly once for the whole app.
import initSqlJs from 'sql.js';
import sqlWasmUrl from 'sql.js/dist/sql-wasm.wasm?url';

let promise = null;

export const getSQL = () => {
    if (!promise) {
        promise = initSqlJs({ locateFile: () => sqlWasmUrl }).catch((error) => {
            // Do not cache the failure: a transient error would otherwise poison every
            // later call for the rest of the session.
            promise = null;
            throw error;
        });
    }
    return promise;
};
